export const websocket__handleCommand = async (command: string, payload: any) => {
    console.log("Handling command", command, payload);
    switch (command) {
        case "vault__create_new_device":
            console.log("Creating new device", payload);
            break;
        case "vault__update_device":
            console.log("Updating device", payload);
            break;
        case "vault__backup_device":
            console.log("Backing up device", payload);
            break;
        default:
            console.error("Unknown command", command);
    }
}

