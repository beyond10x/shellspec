#!/bin/sh
# A generated crate of this repository is byte for byte what its generator writes.
#
# One line per generated crate, in `lines` below: the directory, a `|`, and the generator line,
# in which `{out}` stands for the directory the generator writes into. A second generated crate
# is one more line and no other edit: nothing else reads the list.
#
#   check-generated.sh           compare each crate with a fresh generation
#   check-generated.sh --write   replace each crate with a fresh generation
#   check-generated.sh --list    print the lines
#
# A fresh generation goes into the ignored target/check-generated/<name>. The comparison covers
# every entry of both directories that is no directory, so what git records of the crate: a file
# that differs, an entry the generator did not write, a file it wrote that the tree lacks, an
# entry of the tree that is no regular file (a symbolic link, also one to an equal file) and an
# executable bit that is not as generated are each a difference. `.ess-output/`, the generator's
# own record of what it wrote, holds an id that is new in each generation; it is left out of the
# comparison and never copied into the tree.
#
# Exit 0: one line `check-generated: <directory>: <n> files as generated` per crate.
# Exit 1: each difference is named (of the files that were not generated, the first 20), with
# the command that regenerates.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$root"

lines() {
  cat << 'LINES'
generated|ess generate --path spec --kind schema --out {out}
crates/shellspec-cli-contract|ess generate cli --path spec --binding spec/cli.yaml --out {out}
LINES
}

mode=${1:-check}
case $mode in
  check | --write) ;;
  --list) lines; exit 0 ;;
  *) echo "check-generated: unknown argument $mode" >&2; exit 2 ;;
esac

# files <directory>: its entries that are no directory, sorted, without the generator's own
# record. A symbolic link is listed as itself, whatever it points to.
files() {
  (cd "$1" && find . ! -type d ! -path './.ess-output/*' | LC_ALL=C sort)
}

# regular <path>: 0 when it is a regular file and no symbolic link to one.
regular() {
  [ ! -L "$1" ] && [ -f "$1" ]
}

# bit <file>: `x` when it is executable, `-` otherwise.
bit() {
  if [ -x "$1" ]; then echo x; else echo -; fi
}

work=target/check-generated
fail=0
while IFS='|' read -r dir generator; do
  [ -n "$dir" ] || continue
  fresh=$work/${dir##*/}
  rm -rf "$fresh"
  mkdir -p "$work"
  line=$(printf '%s\n' "$generator" | sed "s|{out}|$fresh|")
  [ "$line" != "$generator" ] || { echo "check-generated: the line of $dir names no {out}" >&2; exit 1; }
  if ! sh -c "$line" > "$fresh.txt" 2>&1 < /dev/null; then
    cat "$fresh.txt"
    echo "check-generated: $dir: the generator failed: $line" >&2
    exit 1
  fi

  if [ "$mode" = --write ]; then
    rm -rf "$fresh/.ess-output" "$dir"
    mkdir -p "$(dirname -- "$dir")"
    cp -R "$fresh" "$dir"
    echo "check-generated: $dir: $(files "$dir" | grep -c .) files written"
    continue
  fi

  [ -d "$dir" ] || { echo "check-generated: $dir: no such directory; run: task generate"; fail=1; continue; }
  files "$fresh" > "$fresh.generated"
  files "$dir" > "$fresh.tree"
  differences=0
  while IFS= read -r file; do
    if [ ! -e "$dir/$file" ] && [ ! -L "$dir/$file" ]; then
      echo "check-generated: $dir/${file#./}: generated, and not in the tree"
    elif ! regular "$dir/$file"; then
      echo "check-generated: $dir/${file#./}: generated as a regular file, and no regular file in the tree"
    elif ! cmp -s "$fresh/$file" "$dir/$file"; then
      echo "check-generated: $dir/${file#./}: differs from what the generator writes"
    elif [ "$(bit "$fresh/$file")" != "$(bit "$dir/$file")" ]; then
      echo "check-generated: $dir/${file#./}: its executable bit is not as the generator writes it"
    else
      continue
    fi
    differences=$((differences + 1))
  done < "$fresh.generated"
  extra=0
  while IFS= read -r file; do
    if [ -e "$fresh/$file" ] || [ -L "$fresh/$file" ]; then continue; fi
    differences=$((differences + 1))
    extra=$((extra + 1))
    if [ "$extra" -le 20 ]; then
      if regular "$dir/$file"; then kind=''; else kind=', no regular file'; fi
      echo "check-generated: $dir/${file#./}: in the tree$kind, and not generated"
    fi
  done < "$fresh.tree"
  if [ "$extra" -gt 20 ]; then
    echo "check-generated: $dir: $((extra - 20)) more file(s) in the tree, and not generated"
  fi
  if [ "$differences" = 0 ]; then
    echo "check-generated: $dir: $(grep -c . "$fresh.generated") files as generated"
  else
    echo "check-generated: $dir: $differences difference(s). A generated file is never edited: change the specification and run: task generate"
    fail=1
  fi
done << LINES
$(lines)
LINES
exit $fail
