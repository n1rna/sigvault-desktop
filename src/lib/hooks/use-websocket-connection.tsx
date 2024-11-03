import { useState, useCallback } from 'react';
import { toast } from 'react-toastify';
import { invoke } from "@tauri-apps/api/tauri";

export function useWebSocketConnection() {
    const [isConnected, setIsConnected] = useState(false);

    const sendMessage = useCallback(async (message: any) => {
        try {
            await invoke('send_websocket_message', {
                message: JSON.stringify(message)
            });
            return true;
        } catch (error) {
            console.error('Failed to send WebSocket message:', error);
            toast.error('Failed to send message. Please try again.');
            return false;
        }
    }, []);

    const connect = useCallback(async (sessionId: string) => {
        try {
            await invoke('cmd_start_session_websocket_connection', {
                sessionId
            });
            setIsConnected(true);
            return true;
        } catch (error) {
            console.error('Failed to connect to WebSocket:', error);
            toast.error('Failed to connect. Please try again.');
            return false;
        }
    }, []);

    return {
        isConnected,
        connect,
        sendMessage
    };
}