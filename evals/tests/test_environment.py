import sys
from importlib import import_module


def test_python_version():
    assert sys.implementation.name == "cpython"
    assert sys.version_info[:2] == (3, 12)


def test_package_import():
    assert import_module("gitserious_evals").__name__ == "gitserious_evals"
