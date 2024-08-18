use std::env::consts::OS;

use machine_uid;

pub struct MachineInformation {
    pub machine_id: String,
    pub machine_type: String,
}

pub fn get_machine_information() -> MachineInformation {
    let machine_id = machine_uid::get().unwrap();
    let machine_type = get_machine_type();
    MachineInformation {
        machine_id,
        machine_type,
    }
}

fn get_machine_type() -> String {
    match OS {
        "windows" => "WindowsDesktop".to_string(),
        "macos" => "MacOSDesktop".to_string(),
        "linux" => "LinuxDesktop".to_string(),
        _ => format!("Other OS: {}", OS),
    }
}
