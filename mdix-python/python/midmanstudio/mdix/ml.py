"""
ML additions for midmanstudio.mdix.

Soft dependencies — only imported when actually used:
  numpy   >= 1.21   (MdixNumpy, MdixTensor)
  pandas  >= 1.3    (MdixDataFrame)

Install with extras:
  pip install midmanstudio-mdix[numpy]
  pip install midmanstudio-mdix[pandas]
  pip install midmanstudio-mdix[ml]      # numpy + pandas
"""

from __future__ import annotations

import base64
import json
from typing import (
    TYPE_CHECKING,
    Any,
    Dict,
    List,
    Optional,
    Sequence,
    Tuple,
    Union,
)

if TYPE_CHECKING:
    import numpy as np
    import pandas as pd
    from . import MdixDatabase, MdixBuilder, MdixResult


# ── Lazy import helpers ────────────────────────────────────────────────────────

def _require_numpy() -> Any:
    try:
        import numpy as np
        return np
    except ImportError as exc:
        raise ImportError(
            "numpy is required for this operation. "
            "Install it with:  pip install midmanstudio-mdix[numpy]"
        ) from exc


def _require_pandas() -> Any:
    try:
        import pandas as pd
        return pd
    except ImportError as exc:
        raise ImportError(
            "pandas is required for this operation. "
            "Install it with:  pip install midmanstudio-mdix[pandas]"
        ) from exc


# ── MdixNumpy ──────────────────────────────────────────────────────────────────

class MdixNumpy:
    """
    Store and retrieve NumPy arrays in `.mdix` databases.

    Arrays are stored as a single flat string property containing a compact
    JSON envelope:
        {"dtype":"float32","ndim":2,"shape":[16,8],"size":128,"order":"C",
         "data":"BASE64..."}

    Using a flat (tier-1) string property avoids two problems that occur with
    tier-2 table properties:
      1. A runtime parser bug that causes very long table property lines (> ~400
         chars) to be silently dropped when no tier-1 flat properties precede
         them in the DATA section.
      2. The tier-2 ordering constraint: after storing a numpy array via a table
         property, no further tier-1 flat properties can be added.

    Flat string properties have neither limitation.

    Usage::

        import numpy as np
        from midmanstudio.mdix import MdixBuilder, MdixDatabase
        from midmanstudio.mdix.ml import MdixNumpy

        arr = np.random.rand(784, 256).astype(np.float32)

        db = (MdixBuilder()
              .set_string("model_name", "my_model")
              )
        db = MdixNumpy.store(db, "weights", arr).to_database()

        restored = MdixNumpy.load(db, "weights")
        assert restored.shape == (784, 256)
    """

    @staticmethod
    def store(
        builder: "MdixBuilder",
        path: str,
        array: "np.ndarray",
        order: str = "C",
    ) -> "MdixBuilder":
        """
        Add a NumPy array to the builder as a flat string property.

        The entire array — metadata and base64-encoded bytes — is serialised
        into a single compact JSON string stored at ``path``.  Because it is a
        tier-1 flat property it can be added before or alongside other flat
        properties, and does not trigger the two-tier ordering constraint.

        Args:
            builder: The ``MdixBuilder`` to add the array to.
            path:    Dotted path where the array will be stored.
            array:   The NumPy array to store.
            order:   Memory layout ``"C"`` (row-major) or ``"F"`` (Fortran).

        Returns:
            The same builder for chaining.
        """
        np = _require_numpy()

        if path.strip() == "":
            raise ValueError("[mdix:ml] Array path cannot be empty")
        if not isinstance(array, np.ndarray):
            raise TypeError(
                f"[mdix:ml] Expected numpy.ndarray, got {type(array).__name__}"
            )

        contiguous = np.ascontiguousarray(array) if order == "C" \
                     else np.asfortranarray(array)
        raw_bytes = contiguous.tobytes(order=order)
        b64_data  = base64.b64encode(raw_bytes).decode("ascii")

        envelope = {
            "dtype":  str(array.dtype),
            "ndim":   array.ndim,
            "shape":  list(array.shape),
            "size":   array.size,
            "order":  order,
            "data":   b64_data,
        }

        return builder.set_string(path, json.dumps(envelope, separators=(",", ":")))

    @staticmethod
    def load(
        db: "MdixDatabase",
        path: str,
        dtype: Optional[Any] = None,
    ) -> "np.ndarray":
        """
        Retrieve a NumPy array from a loaded database.

        Args:
            db:    The loaded ``MdixDatabase``.
            path:  Dotted path where the array is stored.
            dtype: If given, the loaded array is cast to this dtype via
                   ``.astype(dtype)`` after reconstruction.  The stored bytes
                   are always decoded using the original stored dtype first,
                   then converted — this preserves value semantics rather than
                   reinterpreting raw bytes.

        Returns:
            The reconstructed NumPy array.

        Raises:
            MdixError: If the path does not exist or metadata is corrupt.
        """
        np = _require_numpy()

        raw = db.get_string(path)

        try:
            envelope = json.loads(raw)
        except json.JSONDecodeError as exc:
            from . import MdixError
            raise MdixError(
                f"[mdix:ml] Failed to parse numpy envelope at '{path}': {exc}"
            ) from exc

        stored_dtype = envelope["dtype"]
        shape        = tuple(envelope["shape"])
        order_str    = envelope.get("order", "C")
        b64_data     = envelope["data"]

        raw_bytes = base64.b64decode(b64_data)
        arr = (
            np.frombuffer(raw_bytes, dtype=np.dtype(stored_dtype))
              .reshape(shape)
              .copy(order=order_str)  # type: ignore[arg-type]
        )

        # Cast to the requested dtype AFTER reconstruction with the correct
        # element count.  Do NOT reinterpret bytes with the new dtype because
        # that changes the element count (e.g. float32→float64 halves it).
        if dtype is not None and np.dtype(dtype) != np.dtype(stored_dtype):
            arr = arr.astype(dtype)

        return arr

    @staticmethod
    def try_load(
        db: "MdixDatabase",
        path: str,
        dtype: Optional[Any] = None,
    ) -> "MdixResult":
        """Railway variant of :meth:`load` — never raises."""
        from . import MdixResult
        try:
            return MdixResult.ok(MdixNumpy.load(db, path, dtype=dtype))
        except Exception as exc:
            return MdixResult.err(str(exc))

    @staticmethod
    def exists(db: "MdixDatabase", path: str) -> bool:
        """Returns ``True`` if a numpy array is stored at ``path``."""
        if not db.exists(path):
            return False
        # Confirm it is actually a numpy envelope rather than an unrelated
        # string property at the same path.
        if db.get_type(path) != "string":
            return False
        try:
            envelope = json.loads(db.get_string(path))
            return "dtype" in envelope and "data" in envelope
        except Exception:
            return False

    @staticmethod
    def array_info(db: "MdixDatabase", path: str) -> Dict[str, Any]:
        """
        Return metadata about the stored array without decoding its data.

        Returns a dict with keys: ``dtype``, ``ndim``, ``shape``, ``size``,
        ``order``.
        """
        if not db.exists(path):
            return {"dtype": "unknown", "ndim": 0, "shape": "", "size": 0,
                    "order": "C"}
        try:
            envelope = json.loads(db.get_string(path))
            return {
                "dtype": envelope.get("dtype", "unknown"),
                "ndim":  envelope.get("ndim",  0),
                "shape": ",".join(str(s) for s in envelope.get("shape", [])),
                "size":  envelope.get("size",  0),
                "order": envelope.get("order", "C"),
            }
        except Exception:
            return {"dtype": "unknown", "ndim": 0, "shape": "", "size": 0,
                    "order": "C"}


# ── MdixTensor ─────────────────────────────────────────────────────────────────

class MdixTensor:
    """
    Framework-agnostic tensor storage backed by NumPy serialisation.

    Converts PyTorch tensors, TensorFlow tensors, and JAX arrays to NumPy
    before storage, and reconstructs them on load.

    Usage::

        import torch
        from midmanstudio.mdix.ml import MdixTensor

        t = torch.randn(128, 256)

        builder = MdixBuilder().set_string("model", "bert")
        builder = MdixTensor.store(builder, "encoder_weights", t)
        db = builder.to_database()

        restored_torch = MdixTensor.load_torch(db, "encoder_weights")
    """

    @staticmethod
    def _to_numpy(tensor: Any) -> "np.ndarray":
        np = _require_numpy()

        type_name = type(tensor).__module__ + "." + type(tensor).__name__

        if isinstance(tensor, np.ndarray):
            return tensor

        # PyTorch
        if "torch" in type_name:
            return tensor.detach().cpu().numpy()

        # TensorFlow / Keras
        if "tensorflow" in type_name or "keras" in type_name:
            return tensor.numpy()

        # JAX
        if "jax" in type_name:
            return np.array(tensor)

        # Last resort
        return np.asarray(tensor)

    @staticmethod
    def store(
        builder: "MdixBuilder",
        path: str,
        tensor: Any,
        order: str = "C",
    ) -> "MdixBuilder":
        """Store any supported tensor type as a flat string property."""
        arr = MdixTensor._to_numpy(tensor)
        return MdixNumpy.store(builder, path, arr, order=order)

    @staticmethod
    def load_numpy(db: "MdixDatabase", path: str) -> "np.ndarray":
        """Load the stored tensor as a NumPy array."""
        return MdixNumpy.load(db, path)

    @staticmethod
    def load_torch(db: "MdixDatabase", path: str) -> Any:
        """Load as a PyTorch ``Tensor``. Requires ``torch``."""
        try:
            import torch
        except ImportError as exc:
            raise ImportError("torch is required for load_torch.") from exc
        arr = MdixNumpy.load(db, path)
        return torch.from_numpy(arr.copy())

    @staticmethod
    def load_tf(db: "MdixDatabase", path: str) -> Any:
        """Load as a TensorFlow ``Tensor``. Requires ``tensorflow``."""
        try:
            import tensorflow as tf
        except ImportError as exc:
            raise ImportError("tensorflow is required for load_tf.") from exc
        arr = MdixNumpy.load(db, path)
        return tf.constant(arr)


# ── MdixDataFrame ──────────────────────────────────────────────────────────────

class MdixDataFrame:
    """
    Store and retrieve Pandas DataFrames in `.mdix` databases.

    DataFrames are stored as a single flat string property containing a JSON
    array of row objects (orient="records").  This is more robust than using
    DixScript group arrays because it does not rely on the runtime's bracket-
    index key access semantics (``records[0].name`` etc.), which are not
    guaranteed to be stable across runtime versions.

    Usage::

        import pandas as pd
        from midmanstudio.mdix import MdixBuilder, MdixDatabase
        from midmanstudio.mdix.ml import MdixDataFrame

        df = pd.DataFrame({"name": ["Goblin", "Orc"], "hp": [50, 100]})

        builder = MdixBuilder().set_string("dataset", "enemies")
        builder = MdixDataFrame.store(builder, "records", df)
        db = builder.to_database()

        restored = MdixDataFrame.load(db, "records")
    """

    @staticmethod
    def store(
        builder: "MdixBuilder",
        path: str,
        df: "pd.DataFrame",
        orient: str = "records",
    ) -> "MdixBuilder":
        """
        Add a DataFrame to the builder as a flat string property.

        The DataFrame is serialised to a JSON array of row objects and stored
        at ``path`` as a single string value.  As a tier-1 flat property it
        does not trigger the two-tier ordering constraint.

        Args:
            builder: The ``MdixBuilder`` to add the frame to.
            path:    Dotted path for the stored data.
            df:      The Pandas ``DataFrame`` to store.
            orient:  Only ``"records"`` is currently supported.

        Returns:
            The same builder for chaining.
        """
        pd = _require_pandas()

        if not isinstance(df, pd.DataFrame):
            raise TypeError(
                f"[mdix:ml] Expected pandas.DataFrame, got {type(df).__name__}"
            )
        if df.empty:
            raise ValueError("[mdix:ml] Cannot store an empty DataFrame")

        rows     = df.to_dict(orient="records")
        json_str = json.dumps(rows, default=str, separators=(",", ":"))

        return builder.set_string(path, json_str)

    @staticmethod
    def load(
        db: "MdixDatabase",
        path: str,
        columns: Optional[List[str]] = None,
    ) -> "pd.DataFrame":
        """
        Retrieve a DataFrame from a loaded database.

        Args:
            db:      The loaded ``MdixDatabase``.
            path:    Dotted path where the frame is stored.
            columns: If given, only these columns are included in the result.

        Returns:
            A Pandas ``DataFrame``.
        """
        pd = _require_pandas()

        # New format: single flat string property containing JSON
        if db.exists(path) and db.get_type(path) == "string":
            raw = db.get_string(path)
            try:
                rows = json.loads(raw)
                if isinstance(rows, list):
                    df = pd.DataFrame(rows)
                    if columns is not None:
                        df = df[[c for c in columns if c in df.columns]]
                    return df
            except (json.JSONDecodeError, ValueError):
                pass  # fall through to legacy group-array path

        # Legacy format: group array (kept for backward compatibility)
        length = db.get_array_length(path)
        if length < 0:
            raise ValueError(
                f"[mdix:ml] No DataFrame found at '{path}'. "
                "Expected a value stored by MdixDataFrame.store()."
            )

        rows = []
        for i in range(length):
            prefix = f"{path}[{i}]"
            keys   = db.get_keys(prefix)
            row    = {k: _read_scalar(db, f"{prefix}.{k}") for k in keys}
            rows.append(row)

        df = pd.DataFrame(rows)
        if columns is not None:
            df = df[[c for c in columns if c in df.columns]]
        return df

    @staticmethod
    def try_load(
        db: "MdixDatabase",
        path: str,
        columns: Optional[List[str]] = None,
    ) -> "MdixResult":
        """Railway variant of :meth:`load` — never raises."""
        from . import MdixResult
        try:
            return MdixResult.ok(MdixDataFrame.load(db, path, columns=columns))
        except Exception as exc:
            return MdixResult.err(str(exc))


def _read_scalar(db: "MdixDatabase", path: str) -> Any:
    """Read any scalar value from ``path`` without knowing its type upfront."""
    t = db.get_type(path)
    if t == "int":    return db.get_int(path)
    if t == "bool":   return db.get_bool(path)
    if t == "float":  return db.get_float(path)
    if t == "double": return db.get_double(path)
    return db.get_string(path, None)


# ── MdixMLConfig ───────────────────────────────────────────────────────────────

class MdixMLConfig:
    """
    Typed ML configuration wrapper around a loaded ``MdixDatabase``.

    Provides domain-specific helpers for hyperparameters, architecture
    settings, dataset paths, and training metadata — all with validation,
    defaults, and range checking.

    Usage::

        from midmanstudio.mdix import MdixDatabase
        from midmanstudio.mdix.ml import MdixMLConfig

        db     = MdixDatabase.load("run_config.mdix")
        config = MdixMLConfig(db)

        lr     = config.hyperparameter("learning_rate", default=1e-3,
                                        min_val=1e-7, max_val=1.0)
        hidden = config.architecture("hidden_size", default=256,
                                      choices=[128, 256, 512, 1024])
        data   = config.dataset_path("train_data", required=True)
        epochs = config.training("epochs", default=10, dtype=int)
    """

    def __init__(self, db: "MdixDatabase") -> None:
        self._db = db

    @classmethod
    def load(cls, path: str) -> "MdixMLConfig":
        """Load a `.mdix` file and wrap it as an ``MdixMLConfig``."""
        from . import MdixDatabase
        return cls(MdixDatabase.load(path))

    @classmethod
    def load_str(cls, source: str) -> "MdixMLConfig":
        """Parse a raw `.mdix` string and wrap it as an ``MdixMLConfig``."""
        from . import MdixDatabase
        return cls(MdixDatabase.load_str(source))

    @classmethod
    def builder(cls) -> "MdixBuilder":
        """Return a fresh ``MdixBuilder`` ready for ML config construction."""
        from . import MdixBuilder
        return MdixBuilder()

    def database(self) -> "MdixDatabase":
        """The underlying ``MdixDatabase``."""
        return self._db

    # ── Typed accessors ──────────────────────────────────────────────────────

    def hyperparameter(
        self,
        path: str,
        *,
        default: Union[float, int],
        min_val: Optional[float] = None,
        max_val: Optional[float] = None,
        dtype: type = float,
    ) -> Union[float, int]:
        """Read a numeric hyperparameter with optional range validation."""
        if self._db.exists(path):
            val = (self._db.get_int(path)
                   if dtype is int
                   else self._db.get_double(path))
        else:
            val = default

        if min_val is not None and val < min_val:
            raise ValueError(
                f"[mdix:ml] Hyperparameter '{path}' = {val} is below "
                f"minimum {min_val}"
            )
        if max_val is not None and val > max_val:
            raise ValueError(
                f"[mdix:ml] Hyperparameter '{path}' = {val} exceeds "
                f"maximum {max_val}"
            )
        return dtype(val)

    def architecture(
        self,
        path: str,
        *,
        default: Any,
        choices: Optional[Sequence[Any]] = None,
    ) -> Any:
        """Read an architecture setting with optional enumeration validation."""
        if not self._db.exists(path):
            return default

        t   = self._db.get_type(path)
        val = (
            self._db.get_int(path)    if t == "int"    else
            self._db.get_bool(path)   if t == "bool"   else
            self._db.get_double(path) if t in ("float", "double") else
            self._db.get_string(path)
        )

        if choices is not None and val not in choices:
            raise ValueError(
                f"[mdix:ml] Architecture setting '{path}' = {val!r} "
                f"is not in allowed choices: {choices}"
            )
        return val

    def dataset_path(
        self,
        path: str,
        *,
        required: bool = False,
        default: Optional[str] = None,
    ) -> Optional[str]:
        """Read a dataset file or directory path."""
        if self._db.exists(path):
            return self._db.get_string(path)
        if required and default is None:
            from . import MdixError
            raise MdixError(
                f"[mdix:ml] Required dataset path '{path}' not found in config"
            )
        return default

    def training(
        self,
        path: str,
        *,
        default: Any,
        dtype: type = int,
    ) -> Any:
        """Read a training setting (epochs, batch size, seed, etc.)."""
        if not self._db.exists(path):
            return default

        if dtype is int:   return self._db.get_int(path)
        if dtype is float: return self._db.get_double(path)
        if dtype is bool:  return self._db.get_bool(path)
        return self._db.get_string(path)

    def label_map(self, path: str) -> Dict[str, int]:
        """
        Read a label-to-integer mapping.
        Supports both array-of-objects and flat key→int formats.
        """
        result: Dict[str, int] = {}

        length = self._db.get_array_length(path)
        if length >= 0:
            for i in range(length):
                prefix = f"{path}[{i}]"
                name   = self._db.get_string(f"{prefix}.name", "")
                idx    = self._db.get_int(f"{prefix}.id", i)
                if name:
                    result[name] = idx
            return result

        for key in self._db.get_keys(path):
            full = f"{path}.{key}"
            if self._db.get_type(full) == "int":
                result[key] = self._db.get_int(full)
        return result

    def all_hyperparameters(
        self, prefix: str = "hyperparameters"
    ) -> Dict[str, Any]:
        """Return all values under ``prefix`` as a plain dict."""
        out: Dict[str, Any] = {}
        for key in self._db.get_keys(prefix):
            full = f"{prefix}.{key}"
            t    = self._db.get_type(full)
            out[key] = (
                self._db.get_int(full)    if t == "int"    else
                self._db.get_bool(full)   if t == "bool"   else
                self._db.get_double(full) if t in ("float", "double") else
                self._db.get_string(full)
            )
        return out

    # ── NumPy shortcuts ───────────────────────────────────────────────────────

    def load_weights(self, path: str) -> "np.ndarray":
        """Load a NumPy array stored at ``path``."""
        return MdixNumpy.load(self._db, path)

    def weights_info(self, path: str) -> Dict[str, Any]:
        """Return metadata about a stored array without loading its data."""
        return MdixNumpy.array_info(self._db, path)

    # ── Dunder ────────────────────────────────────────────────────────────────

    def __repr__(self) -> str:
        count = self._db.entry_count if self._db.is_valid else -1
        return f"MdixMLConfig(entries={count})"


# ── Public re-exports ──────────────────────────────────────────────────────────

__all__ = [
    "MdixNumpy",
    "MdixTensor",
    "MdixDataFrame",
    "MdixMLConfig",
]
