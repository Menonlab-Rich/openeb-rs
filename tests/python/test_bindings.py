"""Python API tests for the openevt extension module.

These tests load the debug extension produced by ``cargo build`` directly, so
they do not require maturin or a separately installed wheel.
"""

import asyncio
import importlib.util
import os
import shutil
import subprocess
import sys
from pathlib import Path

import numpy as np
import pytest


ROOT = Path(__file__).parents[2]
SAMPLE_RAW = ROOT / "tests" / "sample.raw"


@pytest.fixture(scope="session")
def openevt():
    subprocess.run(
        ["cargo", "build", "--quiet", "--features", "python"],
        cwd=ROOT,
        check=True,
    )
    extension = next((ROOT / "target" / "debug").glob("libopenevt*.so"))

    # The Python extension and the raw-file plugin are built from the same
    # crate, but the plugin must be loaded as a separate shared-library
    # instance across the ABI boundary.
    plugin_dir = ROOT / "target" / "debug" / "python-plugin"
    plugin_dir.mkdir(exist_ok=True)
    shutil.copy2(extension, plugin_dir / "libopenevt_raw_file.so")
    os.environ["OPENEVT_PLUGIN_PATH"] = str(plugin_dir)

    spec = importlib.util.spec_from_file_location("openevt", extension)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    sys.modules["openevt"] = module
    spec.loader.exec_module(module)
    return module


def test_module_registers_binding_classes(openevt):
    assert openevt.RawFileReader
    assert openevt.CDEventReceiver
    assert openevt.CDEventIterator
    assert openevt.AsyncCDEventIterator


def test_reader_lifecycle_and_metadata(openevt):
    reader = openevt.RawFileReader(str(SAMPLE_RAW))
    assert reader.ready()
    assert reader.t_min() is None
    assert reader.t_max() is None


def test_sync_iterator_is_pythonic_and_returns_structured_numpy(openevt):
    reader = openevt.RawFileReader(str(SAMPLE_RAW))
    iterator = reader.iter()

    assert iter(iterator) is iterator
    assert iterator.shape() == (720, 1280)

    batch = next(iterator)
    assert isinstance(batch, np.ndarray)
    assert batch.dtype.names == ("x", "y", "p", "t")
    assert batch.dtype.itemsize == 16
    assert batch.dtype.fields["x"][1] == 0
    assert batch.dtype.fields["y"][1] == 2
    assert batch.dtype.fields["p"][1] == 4
    assert batch.dtype.fields["t"][1] == 8
    assert batch.size > 0
    assert batch["x"].dtype == np.dtype("u2")
    assert batch["p"].dtype == np.dtype("u1")
    assert batch["t"].dtype == np.dtype("u8")


def test_sync_iterator_exposes_batch_and_time_windows(openevt):
    reader = openevt.RawFileReader(str(SAMPLE_RAW))
    iterator = reader.iter()

    batch = iterator.next_batch()
    assert isinstance(batch, np.ndarray)
    assert batch.size > 0

    time_window = iterator.next_delta(10_000)
    assert isinstance(time_window, np.ndarray)
    assert time_window.dtype.names == ("x", "y", "p", "t")
    assert np.all(time_window["t"] < batch["t"][-1] + 10_000)


def test_sync_iterator_next_n_preserves_remaining_events(openevt):
    reader = openevt.RawFileReader(str(SAMPLE_RAW))
    iterator = reader.iter()

    first = iterator.next_n(10)
    second = iterator.next_n(10)

    assert first.size == 10
    assert second.size == 10
    assert first["t"][-1] <= second["t"][0]


def test_receiver_try_recv_returns_a_numpy_batch(openevt):
    reader = openevt.RawFileReader(str(SAMPLE_RAW))
    receiver = reader.cd_receiver()

    assert receiver.try_recv() is None
    reader.load_batch()

    batch = receiver.try_recv()
    assert isinstance(batch, np.ndarray)
    assert batch.dtype.names == ("x", "y", "p", "t")
    assert batch.size > 0


def test_async_iterator_protocol_and_batch(openevt):
    async def collect_one_batch():
        reader = openevt.RawFileReader(str(SAMPLE_RAW))
        iterator = reader.aiter()

        assert iterator.__aiter__() is iterator

        # IterAsync expects another producer to decode raw data.
        reader.load_batch()
        batch = await asyncio.wait_for(anext(iterator), timeout=10)
        return batch

    batch = asyncio.run(collect_one_batch())
    assert isinstance(batch, np.ndarray)
    assert batch.dtype.names == ("x", "y", "p", "t")
    assert batch.size > 0


def test_receiver_async_iteration(openevt):
    async def collect_one_batch():
        reader = openevt.RawFileReader(str(SAMPLE_RAW))
        receiver = reader.cd_receiver()
        reader.load_batch()
        batch = await asyncio.wait_for(anext(receiver), timeout=10)
        return batch

    batch = asyncio.run(collect_one_batch())
    assert isinstance(batch, np.ndarray)
    assert batch.dtype.names == ("x", "y", "p", "t")
    assert batch.size > 0
