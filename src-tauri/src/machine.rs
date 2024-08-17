use machine_uid;

pub fn get_machine_information() -> String {
    let machine_id = machine_uid::get().unwrap();
    machine_id
}
