"""
CLI entry point for adrlint
"""

import sys

from .downloader import run_adrlint


def main():
    """Main entry point for the CLI."""
    run_adrlint(sys.argv[1:])


if __name__ == "__main__":
    main()
