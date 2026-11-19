//! `validate` is the check CI runs on the authored files a pull request changes.
//! It has to name the offending file, exit non-zero, and — because a key is part
//! of the file's name — look past the file it was handed when the directory is
//! what makes it wrong.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const CREDENTIAL: &str = "c7ab6ff0df90974fa01fcd7971336791207c475324f1b288e5808e5f";
const OTHER: &str = "b0ebdd68bd00f3a633c5097fe0243d69fb27abacd7f6959bec63c4c4";
const STAKE: &str = "stake1uy50zl7a9k9c74v66c0gn833at5sh83qnjldk8hg4rrv05g3mmskr";

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

fn service(source: &str) -> String {
    format!(
        r#"stake = "{STAKE}"
label = "Anvil"
kind = "minting_provider"
source = "{source}"
"#
    )
}

fn scratch(name: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "address-registry-validate-{}-{name}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&root);
    for dir in ["contracts", "stakes", "stake-exclusions"] {
        fs::create_dir_all(root.join(dir)).expect("the scratch directories are writable");
    }
    root
}

fn run(root: &Path, files: &[PathBuf]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_address-registry"))
        .arg("validate")
        .arg("--contracts")
        .arg(root.join("contracts"))
        .arg("--stakes")
        .arg(root.join("stakes"))
        .arg("--stake-exclusions")
        .arg(root.join("stake-exclusions"))
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
    assert!(stderr(&output).contains("1 of 1 authored files invalid"));
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
fn a_named_file_outside_the_registry_directories_is_refused() {
    let root = scratch("outside");
    let path = root.join(format!("{CREDENTIAL}.toml"));
    fs::write(&path, contract(CREDENTIAL, lock_use())).expect("write");

    // A file that is in no authored directory is read by no resolver, so
    // accepting it would be the silent no-op this command exists to prevent.
    let output = run(&root, std::slice::from_ref(&path));
    assert!(!output.status.success());
    assert!(stderr(&output).contains("is not in"), "{}", stderr(&output));
}

#[test]
fn a_named_file_that_is_not_a_toml_contract_is_refused() {
    let root = scratch("extension");
    let path = root.join("contracts").join(format!("{CREDENTIAL}.txt"));
    fs::write(&path, contract(CREDENTIAL, lock_use())).expect("write");

    let output = run(&root, std::slice::from_ref(&path));
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
    let output = run(&root, std::slice::from_ref(&same));
    assert!(output.status.success(), "{}", stdout(&output));
    assert!(
        stdout(&output).contains("authored files valid: 1"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn a_valid_stake_service_passes_and_is_named() {
    let root = scratch("stake-valid");
    let path = root.join("stakes").join(format!("{STAKE}.toml"));
    fs::write(&path, service("test")).expect("write");

    let output = run(&root, &[]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stdout(&output).contains(&format!("ok    {}", path.display())),
        "{}",
        stdout(&output)
    );
}

#[test]
fn a_stake_service_with_no_source_is_named_and_refused() {
    let root = scratch("stake-invalid");
    let path = root.join("stakes").join(format!("{STAKE}.toml"));
    fs::write(&path, service(" ")).expect("write");

    let output = run(&root, &[]);
    assert!(!output.status.success());
    let stdout = stdout(&output);
    assert!(stdout.contains(&path.display().to_string()), "{stdout}");
    assert!(stdout.contains("`source` is empty"), "{stdout}");
}

#[test]
fn an_excluded_stake_needs_the_reason_it_is_not_a_service() {
    let root = scratch("exclusion");
    let path = root.join("stake-exclusions").join(format!("{STAKE}.toml"));
    fs::write(&path, format!("stake = \"{STAKE}\"\nreason = \"\"\n")).expect("write");

    let output = run(&root, &[]);
    assert!(!output.status.success());
    assert!(
        stdout(&output).contains("`reason` is empty"),
        "{}",
        stdout(&output)
    );
}
