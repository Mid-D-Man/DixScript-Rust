#!/usr/bin/env bash
# Run this from the root of your local DixScript-Rust checkout, AFTER
# dropping the updated .gitignore + mdix-vsmac/.gitignore into place.
#
# git rm --cached stops git from tracking these paths going forward but
# leaves the actual files on disk untouched — your local node_modules/,
# out/, etc. keep working exactly as before, they just stop showing up
# in `git status` / getting committed.
#
# Note: this does NOT remove the ~589 files from git's HISTORY — old
# commits on GitHub will still contain them. That needs a history rewrite
# (git filter-repo / BFG), which is a separate, more invasive thing —
# only worth doing if repo size/clone time actually becomes a problem.
# This script just stops the bleeding going forward.

set -e

echo "Untracking mdix-vscode/node_modules/ (575 files)..."
git rm -r --cached mdix-vscode/node_modules 2>/dev/null || echo "  (already untracked or path missing)"

echo "Untracking mdix-vscode/out/ (compiled TS output)..."
git rm -r --cached mdix-vscode/out 2>/dev/null || echo "  (already untracked or path missing)"

echo "Untracking mdix-vscode/.rmv/ (stray extension cache)..."
git rm -r --cached mdix-vscode/.rmv 2>/dev/null || echo "  (already untracked or path missing)"

echo "Untracking mdix-vscode/Users/ (stray extension cache)..."
git rm -r --cached mdix-vscode/Users 2>/dev/null || echo "  (already untracked or path missing)"

echo "Untracking mdix-vsmac/.vs/ (Visual Studio local cache)..."
git rm -r --cached mdix-vsmac/.vs 2>/dev/null || echo "  (already untracked or path missing)"

echo ""
echo "Staging the fixed .gitignore files..."
git add .gitignore mdix-vsmac/.gitignore

echo ""
echo "Ready to commit. Review with 'git status', then:"
echo '  git commit -m "chore: stop tracking node_modules/out/.vs/stray cache, fix .gitignore"'
echo "  git push"
