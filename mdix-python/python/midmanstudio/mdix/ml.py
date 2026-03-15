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

    Arrays are stored as table property groups (tier-2) containing:
      dtype, ndim, shape, size, order, data (blob).

    Storage format::

        weights: dtype = "float32", ndim = 2, shape = "784,256",
                 size = 200704, order = "C", data = b:("base64...")

    Usage::

        import numpy as np
        from midmanstudio.mdix import MdixBuilder, MdixDatabase
        from midmanstudio.mdix.ml import MdixNumpy

        arr = np.random.rand(784, 256).astype(np.float32)

        # Store — must be called during tier-2 phase of builder
        builder = (MdixBuilder()
                   .set_string("model_name", "my_model")   # tier 1
                   )
        builder = MdixNumpy.store(builder, "weights", arr)  # tier 2
        db = builder.to_database()

        # Retrieve
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
        Add a NumPy array to the builder as a tier-2 table property group.

        Must be called after all tier-1 flat properties.

        Args:
            builder: The ``MdixBuilder`` to add the array to.
            path:    Dotted path where the array will be stored.
            array:   The NumPy array to store.
            order:   Memory layout ``"C"`` (row-major) or ``"F"`` (column-major).

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

        contiguous = np.ascontiguousarray(array) if order == "C" else np.asfortranarray(array)
        raw_bytes  = contiguous.tobytes(order=order)
        b64_data   = base64.b64encode(raw_bytes).decode("ascii")
        shape_str  = ",".join(str(s) for s in array.shape)

        raw_props: List[Tuple[str, str]] = [
            ("dtype",  f'"{array.dtype}"'),
            ("ndim",   str(array.ndim)),
            ("shape",  f'"{shape_str}"'),
            ("size",   str(array.size)),
            ("order",  f'"{order}"'),
            ("data",   f'b:("{b64_data}")'),
        ]

        return builder._with_raw_table_properties(path, raw_props)

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
            dtype: Override the stored dtype (e.g. ``np.float64``).
                   If ``None``, uses the dtype recorded in the database.

        Returns:
            The reconstructed NumPy array.

        Raises:
            MdixError: If the path does not exist or metadata is corrupt.
        """
        np = _require_numpy()

        stored_dtype = db.get_string(f"{path}.dtype")
        shape_str    = db.get_string(f"{path}.shape")
        ndim         = db.get_int(f"{path}.ndim")
        order_str    = db.get_string(f"{path}.order", "C")
        b64_data     = db.get_string(f"{path}.data")

        # Strip b:("...") wrapper if the runtime returns the raw literal
        if b64_data.startswith('b:("') and b64_data.endswith('")'):
            b64_data = b64_data[4:-2]

        raw_bytes  = base64.b64decode(b64_data)
        used_dtype = dtype if dtype is not None else np.dtype(stored_dtype)

        if shape_str:
            shape = tuple(int(s) for s in shape_str.split(",") if s)
        else:
            shape = (-1,)

        return (
            np.frombuffer(raw_bytes, dtype=used_dtype)
              .reshape(shape)
              .copy(order=order_str)  # type: ignore[arg-type]
        )

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
        """Returns ``True`` if an array is stored at ``path``."""
        return db.exists(f"{path}.dtype") and db.exists(f"{path}.data")

    @staticmethod
    def array_info(db: "MdixDatabase", path: str) -> Dict[str, Any]:
        """
        Returns metadata about the stored array without loading its data.

        Returns a dict with keys: ``dtype``, ``ndim``, ``shape``, ``size``, ``order``.
        """
        return {
            "dtype": db.get_string(f"{path}.dtype", "unknown"),
            "ndim":  db.get_int(f"{path}.ndim",   0),
            "shape": db.get_string(f"{path}.shape", ""),
            "size":  db.get_int(f"{path}.size",   0),
            "order": db.get_string(f"{path}.order", "C"),
        }


# ── MdixTensor ─────────────────────────────────────────────────────────────────

class MdixTensor:
    """
    Framework-agnostic tensor storage backed by NumPy serialization.

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

        # Last resort — try numpy array conversion
        return np.asarray(tensor)

    @staticmethod
    def store(
        builder: "MdixBuilder",
        path: str,
        tensor: Any,
        order: str = "C",
    ) -> "MdixBuilder":
        """Store any supported tensor type as a tier-2 table property group."""
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
        np = _require_numpy()
        arr = MdixNumpy.load(db, path)
        return tf.constant(arr)


# ── MdixDataFrame ──────────────────────────────────────────────────────────────

class MdixDataFrame:
    """
    Store and retrieve Pandas DataFrames in `.mdix` databases.

    DataFrames are stored as group arrays (tier-2)::

        records::
          { col1 = 1, col2 = "alpha" },
          { col1 = 2, col2 = "beta" }

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
        Add a DataFrame to the builder as a tier-2 group array.

        Args:
            builder: The ``MdixBuilder`` to add the frame to.
            path:    Dotted path for the array in the database.
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

        rows = df.to_dict(orient="records")
        return builder.with_group_array(path, rows)

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
            columns: If given, only these columns are included.

        Returns:
            A Pandas ``DataFrame``.
        """
        pd = _require_pandas()

        length = db.get_array_length(path)
        if length < 0:
            raise ValueError(
                f"[mdix:ml] No array found at '{path}' — "
                "expected a group array from MdixDataFrame.store()"
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
    """Read any scalar value from `path` without knowing its type upfront."""
    t = db.get_type(path)
    if t == "int":   return db.get_int(path)
    if t == "bool":  return db.get_bool(path)
    if t == "float": return db.get_float(path)
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

        lr      = config.hyperparameter("learning_rate", default=1e-3,
                                         min_val=1e-7, max_val=1.0)
        hidden  = config.architecture("hidden_size", default=256,
                                       choices=[128, 256, 512, 1024])
        data    = config.dataset_path("train_data", required=True)
        epochs  = config.training("epochs", default=10, dtype=int)
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
        """
        Read a numeric hyperparameter with optional range validation.

        Args:
            path:    Dotted path in the database.
            default: Value returned when the path is absent.
            min_val: Raise ``ValueError`` if value < min_val.
            max_val: Raise ``ValueError`` if value > max_val.
            dtype:   ``float`` or ``int`` — coerces the stored value.
        """
        if self._db.exists(path):
            val = (
                self._db.get_int(path)
                if dtype is int
                else self._db.get_double(path)
            )
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
        """
        Read an architecture setting with optional enumeration validation.

        Args:
            path:    Dotted path in the database.
            default: Value returned when the path is absent.
            choices: If given, raises ``ValueError`` when the value is
                     not in this sequence.
        """
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
        """
        Read a dataset file or directory path.

        Args:
            path:     Dotted path in the database.
            required: Raise ``MdixError`` if absent and no default given.
            default:  Fallback when path is absent and not required.
        """
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
        """
        Read a training setting (epochs, batch size, seed, etc.).

        Args:
            path:    Dotted path in the database.
            default: Value returned when the path is absent.
            dtype:   ``int``, ``float``, ``bool``, or ``str``.
        """
        if not self._db.exists(path):
            return default

        if dtype is int:   return self._db.get_int(path)
        if dtype is float: return self._db.get_double(path)
        if dtype is bool:  return self._db.get_bool(path)
        return self._db.get_string(path)

    def label_map(self, path: str) -> Dict[str, int]:
        """
        Read a label-to-integer mapping stored as an array of table entries.

        Expected format::

            label_map: name = "cat", id = 0
            # OR as flat key-value pairs under a prefix

        Returns a ``{label_name: integer_id}`` dict.
        """
        result: Dict[str, int] = {}

        length = self._db.get_array_length(path)
        if length >= 0:
            for i in range(length):
                prefix = f"{path}[{i}]"
                name   = self._db.get_string(f"{prefix}.name",  "")
                idx    = self._db.get_int(f"{prefix}.id",  i)
                if name:
                    result[name] = idx
            return result

        for key in self._db.get_keys(path):
            full = f"{path}.{key}"
            if self._db.get_type(full) == "int":
                result[key] = self._db.get_int(full)
        return result

    def all_hyperparameters(self, prefix: str = "hyperparameters") -> Dict[str, Any]:
        """
        Return all values under ``prefix`` as a plain dict.

        Useful for logging or passing to experiment trackers.
        """
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

    # ── Numpy shortcuts ───────────────────────────────────────────────────────

    def load_weights(self, path: str) -> "np.ndarray":
        """Load a NumPy array stored at ``path`` (shortcut for ``MdixNumpy.load``)."""
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
