#!/usr/bin/env python3
"""Append a new release to the AppStream metainfo XML before building a release."""

from __future__ import annotations

import argparse
import sys
from datetime import date
from pathlib import Path


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("version", help="Release version (e.g., 1.0.0)")
    parser.add_argument(
        "--metainfo",
        default="data/com.acme.RustGtkExample.metainfo.xml.in.in",
        help="Path to metainfo template",
    )
    args = parser.parse_args()

    metainfo_path = Path(args.metainfo)
    if not metainfo_path.exists():
        sys.exit(f"Error: {metainfo_path} not found")

    content = metainfo_path.read_text()
    release_entry = f'    <release version="{args.version}" date="{date.today()}" />\n'

    # Insert before </releases>
    if "</releases>" in content:
        new_content = content.replace("</releases>", f"  </releases>\n{release_entry}")
    else:
        # No releases tag, create one
        new_content = content.replace(
            '<content_rating type="oars-1.1" />',
            f'<content_rating type="oars-1.1" />\n  <releases>\n{release_entry}  </releases>',
        )

    metainfo_path.write_text(new_content)
    print(f"Added release {args.version} ({date.today()}) to {metainfo_path}")


if __name__ == "__main__":
    main()
