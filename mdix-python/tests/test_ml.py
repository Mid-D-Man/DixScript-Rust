"""
Tests for midmanstudio.mdix.ml — numpy, tensor, pandas, and MLConfig helpers.

All tests are skipped automatically when optional dependencies are absent.
"""

import pytest
import json

numpy   = pytest.importorskip("numpy",   reason="numpy not installed — skipping ML tests")
pandas  = pytest.importorskip("pandas",  reason="pandas not installed — skipping ML tests")

import numpy as np
import pandas as pd

from midmanstudio.mdix import MdixBuilder, MdixDatabase, MdixError
from midmanstudio.mdix.ml import (
    MdixNumpy,
    MdixTensor,
    MdixDataFrame,
    MdixMLConfig,
)


# ── Fixtures ───────────────────────────────────────────────────────────────────

@pytest.fixture
def float32_matrix() -> np.ndarray:
    rng = np.random.default_rng(seed=42)
    return rng.random((16, 8), dtype=np.float32).astype(np.float32)


@pytest.fixture
def int64_vector() -> np.ndarray:
    return np.array([10, 20, 30, 40, 50], dtype=np.int64)


@pytest.fixture
def ml_config_source() -> str:
    return """
    @DATA(
      model_name = "bert-small"
      version    = 1
      hyperparameters: learning_rate = 0.001, batch_size = 32, epochs = 10
      architecture: hidden_size = 256, num_layers = 6, dropout = 0.1
      train_path = "/data/train.csv"
      valid_path = "/data/valid.csv"
    )
    """


@pytest.fixture
def ml_config_db(ml_config_source) -> MdixDatabase:
    db = MdixDatabase.load_str(ml_config_source)
    yield db
    db.close()


# ── MdixNumpy ──────────────────────────────────────────────────────────────────

class TestMdixNumpy:

    def test_store_and_load_float32_matrix(self, float32_matrix):
        db = (MdixBuilder()
              .set_string("model", "test")
              )
        db = MdixNumpy.store(db, "weights", float32_matrix).to_database()

        restored = MdixNumpy.load(db, "weights")
        assert restored.dtype  == np.float32
        assert restored.shape  == float32_matrix.shape
        np.testing.assert_array_almost_equal(restored, float32_matrix)
        db.close()

    def test_store_and_load_int64_vector(self, int64_vector):
        db = MdixNumpy.store(MdixBuilder(), "ids", int64_vector).to_database()

        restored = MdixNumpy.load(db, "ids")
        assert restored.dtype == np.int64
        np.testing.assert_array_equal(restored, int64_vector)
        db.close()

    def test_store_1d_array(self):
        arr = np.array([1.0, 2.0, 3.0], dtype=np.float64)
        db  = MdixNumpy.store(MdixBuilder(), "vec", arr).to_database()
        restored = MdixNumpy.load(db, "vec")
        assert restored.shape == (3,)
        np.testing.assert_array_almost_equal(restored, arr)
        db.close()

    def test_store_3d_array(self):
        arr = np.zeros((2, 3, 4), dtype=np.float32)
        db  = MdixNumpy.store(MdixBuilder(), "cube", arr).to_database()
        restored = MdixNumpy.load(db, "cube")
        assert restored.shape == (2, 3, 4)
        db.close()

    def test_load_with_dtype_override(self, float32_matrix):
        db = MdixNumpy.store(MdixBuilder(), "w", float32_matrix).to_database()
        restored = MdixNumpy.load(db, "w", dtype=np.float64)
        assert restored.dtype == np.float64
        db.close()

    def test_exists_true_after_store(self, float32_matrix):
        db = MdixNumpy.store(MdixBuilder(), "w", float32_matrix).to_database()
        assert MdixNumpy.exists(db, "w") is True
        db.close()

    def test_exists_false_when_absent(self):
        db = MdixBuilder().set_int("x", 1).to_database()
        assert MdixNumpy.exists(db, "w") is False
        db.close()

    def test_array_info_returns_metadata(self, float32_matrix):
        db   = MdixNumpy.store(MdixBuilder(), "w", float32_matrix).to_database()
        info = MdixNumpy.array_info(db, "w")
        assert info["dtype"] == "float32"
        assert info["ndim"]  == 2
        assert "16" in info["shape"]
        assert info["size"]  == float32_matrix.size
        db.close()

    def test_try_load_success(self, float32_matrix):
        db     = MdixNumpy.store(MdixBuilder(), "w", float32_matrix).to_database()
        result = MdixNumpy.try_load(db, "w")
        assert result.is_success
        db.close()

    def test_try_load_failure_missing_path(self):
        db     = MdixBuilder().set_int("x", 1).to_database()
        result = MdixNumpy.try_load(db, "nonexistent")
        assert result.is_failure
        db.close()

    def test_store_non_array_raises(self):
        with pytest.raises(TypeError):
            MdixNumpy.store(MdixBuilder(), "w", [1, 2, 3])

    def test_store_empty_path_raises(self, float32_matrix):
        with pytest.raises(ValueError):
            MdixNumpy.store(MdixBuilder(), "", float32_matrix)

    def test_multiple_arrays_in_one_database(self, float32_matrix, int64_vector):
        b = MdixBuilder().set_string("model", "multi")
        b = MdixNumpy.store(b, "weights", float32_matrix)
        b = MdixNumpy.store(b, "ids",     int64_vector)
        db = b.to_database()

        restored_w = MdixNumpy.load(db, "weights")
        restored_i = MdixNumpy.load(db, "ids")
        assert restored_w.shape == float32_matrix.shape
        assert restored_i.shape == int64_vector.shape
        db.close()


# ── MdixTensor ─────────────────────────────────────────────────────────────────

class TestMdixTensor:

    def test_store_numpy_array(self, float32_matrix):
        db       = MdixTensor.store(MdixBuilder(), "t", float32_matrix).to_database()
        restored = MdixTensor.load_numpy(db, "t")
        assert restored.shape == float32_matrix.shape
        db.close()

    def test_store_and_load_torch(self, float32_matrix):
        torch = pytest.importorskip("torch", reason="torch not installed")
        tensor   = torch.from_numpy(float32_matrix.copy())
        db       = MdixTensor.store(MdixBuilder(), "t", tensor).to_database()
        restored = MdixTensor.load_torch(db, "t")
        assert tuple(restored.shape) == float32_matrix.shape
        db.close()


# ── MdixDataFrame ──────────────────────────────────────────────────────────────

class TestMdixDataFrame:

    def test_store_and_load_basic_frame(self):
        df = pd.DataFrame({
            "name": ["Goblin", "Orc", "Dragon"],
            "hp":   [50,       100,   1000],
        })
        db       = MdixDataFrame.store(MdixBuilder(), "records", df).to_database()
        restored = MdixDataFrame.load(db, "records")
        assert len(restored) == 3
        assert list(restored["name"]) == ["Goblin", "Orc", "Dragon"]
        db.close()

    def test_store_and_load_numeric_columns(self):
        df = pd.DataFrame({
            "x": [1.0, 2.0, 3.0],
            "y": [4.0, 5.0, 6.0],
        })
        db       = MdixDataFrame.store(MdixBuilder(), "points", df).to_database()
        restored = MdixDataFrame.load(db, "points")
        assert len(restored) == 3
        db.close()

    def test_try_load_success(self):
        df       = pd.DataFrame({"a": [1, 2]})
        db       = MdixDataFrame.store(MdixBuilder(), "data", df).to_database()
        result   = MdixDataFrame.try_load(db, "data")
        assert result.is_success
        db.close()

    def test_try_load_failure_missing_path(self):
        db     = MdixBuilder().set_int("x", 1).to_database()
        result = MdixDataFrame.try_load(db, "nonexistent")
        assert result.is_failure
        db.close()

    def test_store_empty_dataframe_raises(self):
        with pytest.raises(ValueError):
            MdixDataFrame.store(MdixBuilder(), "data", pd.DataFrame())

    def test_store_non_dataframe_raises(self):
        with pytest.raises(TypeError):
            MdixDataFrame.store(MdixBuilder(), "data", [1, 2, 3])

    def test_column_filter_on_load(self):
        df = pd.DataFrame({"name": ["A"], "hp": [50], "gold": [10]})
        db = MdixDataFrame.store(MdixBuilder(), "enemies", df).to_database()
        restored = MdixDataFrame.load(db, "enemies", columns=["name", "hp"])
        assert "gold" not in restored.columns
        db.close()


# ── MdixMLConfig ───────────────────────────────────────────────────────────────

class TestMdixMLConfig:

    def test_load_str_and_read_string(self, ml_config_source):
        config = MdixMLConfig.load_str(ml_config_source)
        assert config.database().get_string("model_name") == "bert-small"

    def test_hyperparameter_present(self, ml_config_db):
        config = MdixMLConfig(ml_config_db)
        lr = config.hyperparameter("hyperparameters.learning_rate", default=0.1)
        assert abs(lr - 0.001) < 1e-6

    def test_hyperparameter_missing_uses_default(self, ml_config_db):
        config = MdixMLConfig(ml_config_db)
        val = config.hyperparameter("hyperparameters.missing", default=0.005)
        assert abs(val - 0.005) < 1e-10

    def test_hyperparameter_below_min_raises(self, ml_config_db):
        config = MdixMLConfig(ml_config_db)
        with pytest.raises(ValueError, match="minimum"):
            config.hyperparameter(
                "hyperparameters.learning_rate",
                default=0.1,
                min_val=0.01,
            )

    def test_hyperparameter_above_max_raises(self, ml_config_db):
        config = MdixMLConfig(ml_config_db)
        with pytest.raises(ValueError, match="maximum"):
            config.hyperparameter(
                "hyperparameters.learning_rate",
                default=0.1,
                max_val=0.0001,
            )

    def test_architecture_setting_valid(self, ml_config_db):
        config = MdixMLConfig(ml_config_db)
        hs = config.architecture(
            "architecture.hidden_size",
            default=128,
            choices=[128, 256, 512],
        )
        assert hs == 256

    def test_architecture_invalid_choice_raises(self, ml_config_db):
        config = MdixMLConfig(ml_config_db)
        with pytest.raises(ValueError, match="choices"):
            config.architecture(
                "architecture.hidden_size",
                default=128,
                choices=[64, 128],
            )

    def test_dataset_path_present(self, ml_config_db):
        config = MdixMLConfig(ml_config_db)
        path = config.dataset_path("train_path")
        assert path == "/data/train.csv"

    def test_dataset_path_missing_optional(self, ml_config_db):
        config = MdixMLConfig(ml_config_db)
        path = config.dataset_path("test_path", default="/data/test.csv")
        assert path == "/data/test.csv"

    def test_dataset_path_required_and_missing_raises(self, ml_config_db):
        config = MdixMLConfig(ml_config_db)
        with pytest.raises(MdixError):
            config.dataset_path("nonexistent_path", required=True)

    def test_training_int_setting(self, ml_config_db):
        config = MdixMLConfig(ml_config_db)
        epochs = config.training("hyperparameters.epochs", default=5, dtype=int)
        assert epochs == 10

    def test_training_missing_uses_default(self, ml_config_db):
        config = MdixMLConfig(ml_config_db)
        val = config.training("hyperparameters.warmup_steps", default=100, dtype=int)
        assert val == 100

    def test_repr(self, ml_config_db):
        config = MdixMLConfig(ml_config_db)
        assert "MdixMLConfig" in repr(config)

    def test_weights_round_trip(self):
        arr = np.eye(4, dtype=np.float32)
        b   = MdixBuilder().set_string("model", "identity")
        b   = MdixNumpy.store(b, "kernel", arr)
        db  = b.to_database()

        config = MdixMLConfig(db)
        loaded = config.load_weights("kernel")
        np.testing.assert_array_almost_equal(loaded, arr)
        db.close()

    def test_weights_info(self):
        arr  = np.zeros((10, 20), dtype=np.float32)
        b    = MdixNumpy.store(MdixBuilder(), "w", arr)
        db   = b.to_database()
        config = MdixMLConfig(db)
        info = config.weights_info("w")
        assert info["ndim"] == 2
        db.close()
