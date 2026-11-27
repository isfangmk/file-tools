"""从 CHANGELOG.md 提取指定版本的条目，用作 GitHub Release 说明。

用法：
    python tools/changelog_section.py 0.1.0            # 摘要：标题 + 首个功能小节
    python tools/changelog_section.py 0.1.0 --full     # 完整：该版本的全部条目

摘要模式只取版本标题与紧随其后的第一个小节（通常是"功能"），
适合作为 Release 说明的首屏；完整模式输出该版本的全部内容。

输出为 Markdown，可直接作为 GitHub Release body。若 CHANGELOG 中没有
对应版本则报错退出。
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
CHANGELOG = REPO_ROOT / "CHANGELOG.md"

# 一级版本标题，形如 "## [0.1.0] - 2026-11-27"
_VERSION_HEADING = re.compile(r"^## \[(?P<version>[^\]]+)\](?P<rest>.*)$", re.M)
# 二级小节标题
_SECTION_HEADING = re.compile(r"^### ", re.M)
# 文末的版本链接定义行，形如 "[0.1.0]: https://github.com/..."
_LINK_DEFINITION = re.compile(r"^\[[^\]]+\]:\s+\S+.*$\n?", re.M)


def extract(version: str, changelog: str, *, full: bool = False) -> str:
    """提取某个版本的条目。

    full=False 时取版本标题、引言与紧随其后的第一个小节（通常是"功能"），
    作为 Release 说明的首屏；full=True 时取该版本的全部内容
    （不含下个版本的标题，也不含文末的版本链接定义）。
    """
    matches = list(_VERSION_HEADING.finditer(changelog))
    for index, match in enumerate(matches):
        if match.group("version") != version:
            continue
        start = match.start()
        end = matches[index + 1].start() if index + 1 < len(matches) else len(changelog)
        section = changelog[start:end].rstrip() + "\n"
        # 去掉文末的版本链接定义行（形如 "[0.1.0]: https://..."）——
        # 那是 CHANGELOG 自己的引用语法，放进 Release 说明里没有意义。
        section = _LINK_DEFINITION.sub("", section).rstrip() + "\n"
        if full:
            return section
        # 摘要：保留标题与引言，截到第二个 "### " 之前（即第一个小节结束处）
        headings = list(_SECTION_HEADING.finditer(section))
        if len(headings) < 2:
            return section
        return section[: headings[1].start()].rstrip() + "\n"
    raise KeyError(f"CHANGELOG.md 中没有版本 {version} 的条目")


def main(argv: list[str]) -> int:
    args = [a for a in argv[1:] if not a.startswith("--")]
    full = "--full" in argv
    if len(args) != 1:
        print(__doc__, file=sys.stderr)
        return 2
    version = args[0].lstrip("v")
    try:
        section = extract(version, CHANGELOG.read_text(encoding="utf-8"), full=full)
    except (KeyError, OSError) as exc:
        print(f"无法提取版本 {version} 的变更条目：{exc}", file=sys.stderr)
        return 1
    # Windows 上 stdout 默认是 GBK/cp1252，中文内容无法直接写出。
    sys.stdout.reconfigure(encoding="utf-8")
    sys.stdout.write(section)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
