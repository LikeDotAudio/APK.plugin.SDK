# Part of the APK.audio project — http://APK.audio — made by Anthony Kuzub
# MIT Licence. Free, for everyone, for ever. Full text in LICENSE at the root.
"""CONTRACT_VERSION is one number in two languages (PLAN-3407.01).

The Rust side is `env!("CARGO_PKG_VERSION")` of rust/Cargo.toml; the Python side
is a string literal. A plugin publishes whichever it links as
`build.contract_version`, so the two must never disagree.
"""

import os
import re
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.join(HERE, ".."))

import apkaudio_contracts  # noqa: E402


class ContractVersion(unittest.TestCase):
    def test_python_matches_the_rust_crate(self):
        with open(os.path.join(HERE, "..", "..", "rust", "Cargo.toml"), encoding="utf-8") as handle:
            package = handle.read().split("[package]", 1)[1]
        rust = re.search(r'^version\s*=\s*"([^"]+)"', package, re.M).group(1)
        self.assertEqual(apkaudio_contracts.CONTRACT_VERSION, rust)

    def test_python_package_version_is_the_contract_version(self):
        with open(os.path.join(HERE, "..", "pyproject.toml"), encoding="utf-8") as handle:
            project = handle.read().split("[project]", 1)[1]
        version = re.search(r'^version\s*=\s*"([^"]+)"', project, re.M).group(1)
        self.assertEqual(apkaudio_contracts.CONTRACT_VERSION, version)


if __name__ == "__main__":
    unittest.main()
