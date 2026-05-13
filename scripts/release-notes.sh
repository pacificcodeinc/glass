#!/usr/bin/env sh
set -eu

version="${1:-}"
if [ -z "$version" ]; then
    version="$(awk -F '"' '/^version = / { print $2; exit }' Cargo.toml)"
fi

case "$version" in
    v*) tag="$version" ;;
    *) tag="v$version" ;;
esac

if ! git rev-parse -q --verify "refs/tags/$tag" >/dev/null; then
    echo "error: tag $tag does not exist" >&2
    exit 1
fi

previous_tag="$(
    git tag --merged "$tag" --sort=-v:refname \
        | awk -v tag="$tag" '$0 != tag { print; exit }'
)"

repo_url=""
if [ -n "${GITHUB_REPOSITORY:-}" ]; then
    repo_url="https://github.com/$GITHUB_REPOSITORY"
else
    remote_url="${GITHUB_REPOSITORY_URL:-$(git config --get remote.origin.url || true)}"
    case "$remote_url" in
        git@github.com:*)
            repo_path="${remote_url#git@github.com:}"
            repo_url="https://github.com/${repo_path%.git}"
            ;;
        https://github.com/*)
            repo_url="${remote_url%.git}"
            ;;
    esac
fi

echo "# Release $tag"
echo

if [ -n "$previous_tag" ]; then
    range="$previous_tag..$tag"
    compare="$previous_tag...$tag"

    if [ -n "$repo_url" ]; then
        echo "Compare: $repo_url/compare/$compare"
    else
        echo "Compare: $compare"
    fi
else
    range="$tag"

    if [ -n "$repo_url" ]; then
        echo "Release: $repo_url/releases/tag/$tag"
    fi
fi

echo
echo "## Commits"
echo

git log --reverse --format='%H%x09%h%x09%s' "$range" \
    | awk -F '\t' -v repo_url="$repo_url" '
        repo_url != "" {
            printf "- [`%s`](%s/commit/%s) %s\n", $2, repo_url, $1, $3
            next
        }
        {
            printf "- `%s` %s\n", $2, $3
        }
    '
