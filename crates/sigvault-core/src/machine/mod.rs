// Machine information retrieval module

use std::env::consts::OS;

#[derive(Debug, Clone)]
pub struct MachineInformation {
    pub machine_id: String,
    pub machine_type: String,
}

pub fn get_machine_information() -> MachineInformation {
    let machine_id = machine_uid::get().unwrap_or_else(|_| "unknown".to_string());
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
        _ => format!("OtherOS:{OS}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_machine_type_format() {
        let machine_type = get_machine_type();
        assert!(!machine_type.is_empty());
    }
}
