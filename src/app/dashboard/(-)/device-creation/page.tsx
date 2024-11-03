
"use client";

import { DeviceKeyForm } from '@/components/device/DeviceKeyForm';
import { useWebSocketConnection } from '@/lib/hooks/use-websocket-connection';
import { useAppState } from '@/lib/providers';
import { useRouter } from 'next/navigation';
import { useEffect } from 'react';

export default function DeviceCreationPage() {
    const { applicationState } = useAppState();
    const router = useRouter();
    const { sendMessage } = useWebSocketConnection();

    useEffect(() => {
        // Initialize session when component mounts
        const initializeSession = async () => {
            const message = {
                type: 'session',
                action: 'initialize',
                payload: {}
            };
            await sendMessage(JSON.stringify(message));
        };

        if (applicationState.socket_connected) {
            initializeSession();
        }
    }, [applicationState.socket_connected, sendMessage]);

    // Handle session completion
    useEffect(() => {
        if (applicationState.session_state?.step === 'Completed') {
            router.push('/dashboard/sessions');
        }
    }, [applicationState.session_state?.step, router]);

    return (
        <div className="container mx-auto p-4">
            <h1 className="text-2xl font-bold mb-4">Device Creation</h1>
            <DeviceKeyForm />
        </div>
    );
}