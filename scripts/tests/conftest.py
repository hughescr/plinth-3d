import sys
import os

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

import pytest
from support_placement import mesh_io, config


@pytest.fixture
def cfg():
    return config.SupportConfig()


@pytest.fixture
def box_mesh():
    return mesh_io.make_box(size=(10, 10, 10))


@pytest.fixture
def table_mesh():
    return mesh_io.make_overhang_table()


@pytest.fixture
def fin_mesh():
    return mesh_io.make_thin_fin()
