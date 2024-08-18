export type EventPayload = {
    // Define the structure of your event payload here
    success: boolean,
    error: string,
    message: {
      message_type: string;
      payload: any;
    }
  };
  
export type BackendCommandResult = {
success: boolean;
error?: string;
message: string;
};
