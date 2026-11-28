//! The deployed release and roadmap hand their data to these GitHub Actions.
//! Dependabot may update their immutable commits while each job keeps its role.
#![allow(unexpected_cfgs)]
#[cfg(not(verus_keep_ghost))]
use verus_builtin_macros::verus;
#[cfg(verus_keep_ghost)]
use vstd::prelude::*;

verus! {
fn deployed_roadmap() -> (text: &'static str) { include_str!("../../../.github/workflows/issue-roadmap.yml") }
fn deployed_installer() -> (text: &'static str) { include_str!("../../../.chrono-harness/install-tools.sh") }
fn deployed_roadmap_script() -> (text: &'static str) { include_str!("../../../.github/roadmap.cjs") }
fn deployed_release() -> (text: &'static str) { include_str!("../../../.github/workflows/release.yml") }

spec fn is_hex(c: u8) -> bool { (48 <= c <= 57) || (97 <= c <= 102) }

fn lowercase_hex(c: u8) -> (valid: bool)
    ensures valid == ((48 <= c <= 57) || (97 <= c <= 102))
{
    (48 <= c && c <= 57) || (97 <= c && c <= 102)
}

fn accepts_reference(expected: &String, actual: &String, commit: &[u8]) -> (valid: bool)
    ensures valid == (expected@ == actual@ && commit.len() == 40
        && forall|j: int| 0 <= j < commit.len() ==>
            is_hex(#[trigger] commit@[j]))
{
    if *expected != *actual || commit.len() != 40 { return false; }
    let mut i = 0;
    while i < commit.len()
        invariant i <= commit.len(), forall|j: int| 0 <= j < i ==>
            is_hex(#[trigger] commit@[j]),
        decreases commit.len() - i,
    {
        if !lowercase_hex(commit[i]) { return false; }
        i += 1;
    }
    true
}
}

#[cfg(not(verus_keep_ghost))]
mod native {
    use super::*;
    use serde_yaml::Value;

    fn check_job(workflow: &Value, name: &str, expected: &[&str]) {
        let steps = workflow["jobs"][name]["steps"].as_sequence().unwrap();
        let actions: Vec<_> = steps.iter().filter_map(|step| step.get("uses")).collect();
        assert_eq!(actions.len(), expected.len(), "{name}");
        for (action, expected) in actions.iter().zip(expected) {
            let (repository, commit) = action.as_str().unwrap().split_once('@').unwrap();
            assert!(
                accepts_reference(
                    &expected.to_string(),
                    &repository.to_string(),
                    commit.as_bytes()
                ),
                "{name}: {action:?}"
            );
        }
    }

    #[test]
    fn release_and_roadmap_use_their_registered_immutable_actions() {
        let release: Value = serde_yaml::from_str(deployed_release()).unwrap();
        check_job(&release, "source", &["actions/checkout"]);
        check_job(
            &release,
            "build",
            &["actions/checkout", "actions/upload-artifact"],
        );
        check_job(
            &release,
            "publish",
            &["actions/checkout", "actions/download-artifact"],
        );
        let roadmap: Value = serde_yaml::from_str(deployed_roadmap()).unwrap();
        for job in ["release-issue", "reconcile"] {
            check_job(
                &roadmap,
                job,
                &["actions/checkout", "actions/github-script"],
            );
        }
    }

    #[test]
    fn installer_and_roadmap_script_match_the_published_render() {
        use sha2::{Digest, Sha256};

        let digest = |source: &str| {
            Sha256::digest(source.as_bytes())
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        };
        assert_eq!(
            digest(deployed_installer()),
            "40f5d822dce9ad1e31ccabeed9df7ffcb3b59cffcb000411b13306b05aa6bb72"
        );
        assert_eq!(
            digest(deployed_roadmap_script()),
            "86e97ab0ed63be36d37d32e7d164da4e226c6f616685ad4ce1dd75f814095f8a"
        );
    }

    #[test]
    fn action_binding_rejects_other_repositories_tags_and_invalid_commits() {
        let repository = "actions/checkout".to_owned();
        let commit = b"0123456789abcdef0123456789abcdef01234567";
        assert!(accepts_reference(&repository, &repository, commit));
        assert!(!accepts_reference(
            &repository,
            &"another/checkout".to_owned(),
            commit
        ));
        assert!(!accepts_reference(&repository, &repository, b"v6"));
        let mut invalid = *commit;
        invalid[39] = b'g';
        assert!(!accepts_reference(&repository, &repository, &invalid));
    }
}
