//! Test de bout en bout du binaire `relay-release`.
use std::process::Command;

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_relay-release"))
}

fn run(args: &[&str]) -> (bool, String) {
    let o = bin().args(args).output().unwrap();
    (
        o.status.success(),
        format!(
            "{}{}",
            String::from_utf8_lossy(&o.stdout),
            String::from_utf8_lossy(&o.stderr)
        ),
    )
}

#[test]
fn chaine_cli_complete() {
    let d = tempfile::TempDir::new().unwrap();
    let p = |n: &str| d.path().join(n).to_string_lossy().into_owned();
    std::fs::write(p("relay-setup.exe"), b"faux installeur").unwrap();

    let (ok, pk) = run(&["keygen", "--out", &p("key.seed")]);
    assert!(ok, "{pk}");
    let pk = pk.trim().to_string();
    let plat = ["--platform", "windows-x86_64"];

    let (ok, out) = run(&[
        &["manifest"][..],
        &plat,
        &[
            "--version",
            "1.0.0",
            "--sequence",
            "1",
            "--artifact",
            &p("relay-setup.exe"),
            "--kind",
            "nsis",
            "--url",
            "https://example.invalid/relay-setup.exe",
            "--out",
            &p("m.json"),
        ],
    ]
    .concat());
    assert!(ok, "{out}");
    let (ok, out) = run(&[
        "sign",
        "--key",
        &p("key.seed"),
        "--manifest",
        &p("m.json"),
        "--out",
        &p("m.signed.json"),
    ]);
    assert!(ok, "{out}");
    let (ok, out) = run(&[
        &["verify"][..],
        &plat,
        &[
            "--trust",
            &pk,
            "--signed",
            &p("m.signed.json"),
            "--artifact",
            &p("relay-setup.exe"),
        ],
    ]
    .concat());
    assert!(ok, "{out}");

    // Mauvaise clé de confiance → refus
    let other = hex::encode([7u8; 32]);
    let (ok, _) = run(&[
        &["verify"][..],
        &plat,
        &["--trust", &other, "--signed", &p("m.signed.json")],
    ]
    .concat());
    assert!(!ok);

    let (ok, out) = run(&[
        &["install"][..],
        &plat,
        &[
            "--root",
            &p("root"),
            "--trust",
            &pk,
            "--signed",
            &p("m.signed.json"),
            "--artifact",
            &p("relay-setup.exe"),
        ],
    ]
    .concat());
    assert!(ok, "{out}");
    // Rejeu → refus
    let (ok, out) = run(&[
        &["install"][..],
        &plat,
        &[
            "--root",
            &p("root"),
            "--trust",
            &pk,
            "--signed",
            &p("m.signed.json"),
            "--artifact",
            &p("relay-setup.exe"),
        ],
    ]
    .concat());
    assert!(!ok && out.contains("rejeu"), "{out}");

    // Artefact altéré → refus
    std::fs::write(p("relay-setup.exe"), b"faux installeuR").unwrap();
    let (ok, out) = run(&[
        &["verify"][..],
        &plat,
        &[
            "--trust",
            &pk,
            "--signed",
            &p("m.signed.json"),
            "--artifact",
            &p("relay-setup.exe"),
        ],
    ]
    .concat());
    assert!(!ok && out.contains("hash"), "{out}");
}
