use shellspec::shape;

use crate::zsh;

#[test]
fn shape_keeps_structure_and_drops_values() {
    let input = "cd /srv/acme/secret && API_KEY=hunter2 slot cargo test --features gold -p crate-x 2>&1 | tail -20";
    let s = shape(&zsh(input));
    assert_eq!(
        s,
        "cd <path> && <var>=<v> slot cargo test --features <v> -p <v> 2>&1 | tail -<n>"
    );
    for value in [
        "acme", "secret", "API_KEY", "hunter2", "gold", "crate-x", "20",
    ] {
        assert!(!s.contains(value), "{value} in {s}");
    }
}

#[test]
fn shape_of_unknown_programs_paths_and_substitutions() {
    let s = shape(&zsh(
        "./build.sh --out dist; mytool x; echo \"$(git rev-parse HEAD)\" > /dev/null; cat <<'EOF' > notes.txt\nprivate\nEOF\n",
    ));
    assert_eq!(
        s,
        "<path> --out <arg>; <prog> <arg>; echo $(git rev-parse <arg>) > /dev/null; cat <<'EOF' > <path>"
    );
}

#[test]
fn shape_of_compound_commands() {
    let s = shape(&zsh(
        "for f in *.rs; do wc -l \"$f\"; done | sort -n; if [ -f x ]; then echo; fi",
    ));
    assert_eq!(
        s,
        "for <var> in <$>; do wc -l <$>; done | sort -n; if [ -f <arg> ]; then echo; fi"
    );
}
