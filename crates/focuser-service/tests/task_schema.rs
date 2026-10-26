#![cfg(windows)]

use focuser_service::{installation::*, update_broker::*};
use std::path::Path;

#[test]
fn agent_export_accepts_omitted_default_but_rejects_elevation_and_identity_changes() {
    let image = Path::new(r"C:\Program Files\Focuser\focuser-session-agent.exe");
    let owner = "S-1-5-21-1";
    let xml = agent_task(owner, image, "installation");
    let exported = xml.replace("<RunLevel>LeastPrivilege</RunLevel>", "");
    assert!(task_belongs_to_installation(
        &exported,
        owner,
        image,
        "installation"
    ));
    for altered in [
        xml.replace("LeastPrivilege", "HighestAvailable"),
        xml.replace("LeastPrivilege", ""),
        exported.replace(owner, "S-1-5-21-2"),
        exported.replace("InteractiveToken", "Password"),
        exported.replace("session agent installation", "session agent other"),
        exported.replace("focuser-session-agent.exe", "other.exe"),
        exported.replace("</Command>", "</Command><Arguments>--other</Arguments>"),
    ] {
        assert!(!task_belongs_to_installation(
            &altered,
            owner,
            image,
            "installation"
        ));
    }
}

#[test]
fn system_xml_omits_the_api_only_service_account_logon_type() {
    let broker = Path::new(r"C:\ProgramData\FocuserAuthority\update-broker.exe");
    let xml = recovery_task_xml(broker, "installation");
    assert!(recovery_task_owned(&xml, broker, "installation"));
    for replacement in [
        "<LogonType>ServiceAccount</LogonType>",
        "<LogonType>Password</LogonType>",
        "<LogonType>InteractiveToken</LogonType>",
    ] {
        assert!(!recovery_task_owned(
            &xml.replace("</UserId>", &format!("</UserId>{replacement}")),
            broker,
            "installation"
        ));
    }
    for (original, other) in [
        ("S-1-5-18", "S-1-5-19"),
        ("HighestAvailable", "LeastPrivilege"),
        ("update-broker</Arguments>", "remove</Arguments>"),
        ("PT5M", "PT0S"),
        ("IgnoreNew", "Parallel"),
        ("installation</Description>", "other</Description>"),
    ] {
        assert!(!recovery_task_owned(
            &xml.replace(original, other),
            broker,
            "installation"
        ));
    }
    // Real Task Scheduler exports may rename the principal's identifier.
    let exported = xml
        .replace("id=\"System\"", "id=\"Author\"")
        .replace("Context=\"System\"", "Context=\"Author\"");
    assert!(recovery_task_owned(&exported, broker, "installation"));
}

#[test]
fn generated_tasks_keep_system_and_interactive_session_roles_separate() {
    let (owner, _) = focuser_ipc::windows::current_identity().unwrap();
    let agent_image = Path::new(r"C:\Program Files\Focuser\focuser-session-agent.exe");
    let broker = Path::new(r"C:\ProgramData\FocuserAuthority\update-broker.exe");
    let agent = agent_task(&owner, agent_image, "schema-validation");
    let recovery = recovery_task_xml(broker, "schema-validation");
    assert!(task_belongs_to_installation(
        &agent,
        &owner,
        agent_image,
        "schema-validation"
    ));
    assert!(agent.contains("<LogonType>InteractiveToken</LogonType>"));
    assert!(agent.contains("<RunLevel>LeastPrivilege</RunLevel>"));
    assert!(!recovery.contains("<LogonType>"));
    assert!(recovery_task_owned(&recovery, broker, "schema-validation"));
    if let Some(folder) = std::env::var_os("FOCUSER_TASK_FIXTURE_DIR") {
        let folder = std::path::PathBuf::from(folder);
        std::fs::create_dir_all(&folder).unwrap();
        for (name, xml) in [("agent.xml", agent), ("recovery.xml", recovery)] {
            let bytes: Vec<u8> = std::iter::once(0xfeffu16)
                .chain(xml.encode_utf16())
                .flat_map(u16::to_le_bytes)
                .collect();
            std::fs::write(folder.join(name), bytes).unwrap();
        }
        // The Windows harness writes actual COM-normalized XML without
        // registering a task. Feed it back through the production matchers.
        for (name, agent) in [
            ("agent-exported.xml", true),
            ("recovery-exported.xml", false),
        ] {
            let path = folder.join(name);
            if path.exists() {
                let bytes = std::fs::read(path).unwrap();
                assert!(bytes.starts_with(&[0xff, 0xfe]));
                let words: Vec<_> = bytes[2..]
                    .chunks_exact(2)
                    .map(|b| u16::from_le_bytes([b[0], b[1]]))
                    .collect();
                let xml = String::from_utf16(&words).unwrap();
                if agent {
                    assert!(task_belongs_to_installation(
                        &xml,
                        &owner,
                        agent_image,
                        "schema-validation"
                    ));
                } else {
                    assert!(recovery_task_owned(&xml, broker, "schema-validation"));
                }
            }
        }
    }
}
