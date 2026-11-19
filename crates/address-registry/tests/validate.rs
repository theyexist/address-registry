//! `validate` is the check CI runs on the contract files a pull request
//! changes. It has to name the offending file, exit non-zero, and — because a
//! credential's file name is part of the rule — look past the file it was handed
//! when the directory is what makes it wrong.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const CREDENTIAL: &str = "c7ab6ff0df90974fa01fcd7971336791207c475324f1b288e5808e5f";
const OTHER: &str = "b0ebdd68bd00f3a633c5097fe0243d69fb27abacd7f6959bec63c4c4";

/// A use that is valid on its own: the address decodes to `CREDENTIAL`.
fn lock_use() -> &'static str {
    r#"[[use]]
category = "lock"
shape = "timestamp_and_owner"
source = "test"

[use.derived_from]
address = "addr1w8r6kmlsm7gfwnaqrlxhjufnv7gjqlz82vj0rv5gukqguhcs9tj6j"
"#
}

fn attested_use(reason: &str) -> String {
    format!(
        r#"[[use]]
category = "staking"
shape = "owner_only"
source = "test"

[use.derived_from]
attested = "{reason}"
"#
    )
}

fn contract(credential: &str, body: &str) -> String {
    format!("credential = \"{credential}\"\n\n{body}")
}

fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "address-registry-validate-{}-{name}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("contracts")).expect("the scratch directory is writable");
    root
}

fn run(root: &Path, files: &[PathBuf]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_address-registry"))
        .arg("validate")
        .arg("--contracts")
        .arg(root.join("contracts"))
        .args(files)
        .output()
        .expect("the binary runs")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn a_valid_contract_passes_and_is_named() {
    let root = scratch("valid");
    let path = root.join("contracts").join(format!("{CREDENTIAL}.toml"));
    fs::write(&path, contract(CREDENTIAL, lock_use())).expect("write");

    let output = run(&root, &[]);
    assert!(output.status.success(), "{}", stderr(&output));
    let stdout = stdout(&output);
    assert!(
        stdout.contains(&format!("ok    {}", path.display())),
        "{stdout}"
    );
}

#[test]
fn an_invalid_contract_is_named_with_its_reason() {
    let root = scratch("invalid");
    let path = root.join("contracts").join(format!("{CREDENTIAL}.toml"));
    fs::write(&path, contract(CREDENTIAL, &attested_use(""))).expect("write");

    let output = run(&root, &[]);
    assert!(!output.status.success());
    let stdout = stdout(&output);
    assert!(
        stdout.contains(&format!("FAIL  {}", path.display())),
        "{stdout}"
    );
    assert!(stdout.contains("needs a reason"), "{stdout}");
    assert!(stderr(&output).contains("1 of 1 contract files invalid"));
}

#[test]
fn a_file_named_for_another_credential_is_refused() {
    let root = scratch("filename");
    let path = root.join("contracts").join(format!("{OTHER}.toml"));
    fs::write(&path, contract(CREDENTIAL, lock_use())).expect("write");

    let output = run(&root, &[]);
    assert!(!output.status.success());
    let stdout = stdout(&output);
    assert!(stdout.contains("the file must be"), "{stdout}");
    assert!(stdout.contains(&format!("{CREDENTIAL}.toml")), "{stdout}");
}

#[test]
fn a_named_file_is_checked_against_the_directory() {
    let root = scratch("named");
    let path = root.join("contracts").join(format!("{CREDENTIAL}.toml"));
    fs::write(&path, contract(CREDENTIAL, lock_use())).expect("write");

    // The same credential in a second directory: each file passes on its own,
    // and the pair is the thing `validate` has to refuse. A new file is named
    // explicitly because it is not in the contracts directory until it moves.
    let elsewhere = root.join("elsewhere");
    fs::create_dir_all(&elsewhere).expect("create");
    let copy = elsewhere.join(format!("{CREDENTIAL}.toml"));
    fs::write(&copy, contract(CREDENTIAL, lock_use())).expect("write");

    let output = run(&root, std::slice::from_ref(&copy));
    assert!(!output.status.success());
    let stdout = stdout(&output);
    assert!(stdout.contains(&copy.display().to_string()), "{stdout}");
    assert!(stdout.contains("is also declared by"), "{stdout}");
}

#[test]
fn a_named_file_that_is_not_a_contract_is_refused() {
    let root = scratch("extension");
    let path = root.join(format!("{CREDENTIAL}.txt"));
    fs::write(&path, contract(CREDENTIAL, lock_use())).expect("write");

    let output = run(&root, &[path]);
    assert!(!output.status.success());
    assert!(
        stdout(&output).contains("a contract file is"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn one_file_named_twice_is_one_contract() {
    let root = scratch("twice");
    let path = root.join("contracts").join(format!("{CREDENTIAL}.toml"));
    fs::write(&path, contract(CREDENTIAL, lock_use())).expect("write");

    // The directory holds `contracts/<credential>.toml`; the argument spells
    // the same file `./contracts/<credential>.toml`. Reading it twice would
    // look like two files declaring one credential.
    let same = root
        .join("contracts")
        .join(".")
        .join(format!("{CREDENTIAL}.toml"));
    let output = run(&root, &[same]);
    assert!(output.status.success(), "{}", stdout(&output));
    assert!(
        stdout(&output).contains("1 contract files valid"),
        "{}",
        stdout(&output)
    );
}
