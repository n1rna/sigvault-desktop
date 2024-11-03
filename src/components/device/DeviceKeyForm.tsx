
import * as React from 'react';
import { Card, CardHeader, CardTitle, CardContent } from "@/components/ui/card"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { Input } from "@/components/ui/input"
import { useAppState } from "@/lib/providers"
import { useWebSocketConnection } from '@/lib/hooks/use-websocket-connection';

interface FormData {
    xpub: string;
    fingerprint: string;
    derivation_path: string;
}

const DEFAULT_FORM_DATA: FormData = {
    xpub: '',
    fingerprint: '',
    derivation_path: '',
};

export function DeviceKeyForm() {
    const { sendMessage } = useWebSocketConnection();
    const [formData, setFormData] = React.useState<FormData>(DEFAULT_FORM_DATA);
    const [error, setError] = React.useState<string | null>(null);
    const [isSubmitting, setIsSubmitting] = React.useState(false);

    const handleInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
        const { name, value } = e.target;
        setFormData(prev => ({
            ...prev,
            [name]: value
        }));
        setError(null);
    };

    const validateForm = (): boolean => {
        // Add your validation logic here based on requirements
        const xpubRegex = /^([xtyvz]pub[1-9A-HJ-NP-Za-km-z]{100,108})$/;
        const fingerprintRegex = /^[0-9a-fA-F]{8}$/;
        const derivationPathRegex = /^m(\/\d+'?)+$/;

        if (!xpubRegex.test(formData.xpub)) {
            setError('Invalid xpub format');
            return false;
        }

        if (!fingerprintRegex.test(formData.fingerprint)) {
            setError('Invalid fingerprint format (8 hex characters required)');
            return false;
        }

        if (!derivationPathRegex.test(formData.derivation_path)) {
            setError('Invalid derivation path format');
            return false;
        }

        return true;
    };

    const handleSubmit = async (e: React.FormEvent) => {
        e.preventDefault();
        if (!validateForm()) return;

        setIsSubmitting(true);
        try {
            // Send the form data through WebSocket
            const message = {
                type: 'session',
                action: 'submit',
                payload: formData
            };

            // Here you would send the message through your WebSocket connection
            // This will depend on your WebSocket implementation
            await sendMessage(JSON.stringify(message));

        } catch (err) {
            setError('Failed to submit form. Please try again.');
        } finally {
            setIsSubmitting(false);
        }
    };

    return (
        <Card className="w-full max-w-md mx-auto">
            <CardHeader>
                <CardTitle>Device Key Information</CardTitle>
            </CardHeader>
            <CardContent>
                <form onSubmit={handleSubmit} className="space-y-4">
                    <div className="space-y-2">
                        <Label htmlFor="xpub">Extended Public Key (xpub)</Label>
                        <Input
                            id="xpub"
                            name="xpub"
                            value={formData.xpub}
                            onChange={handleInputChange}
                            placeholder="Enter xpub"
                            required
                        />
                    </div>

                    <div className="space-y-2">
                        <Label htmlFor="fingerprint">Parent Fingerprint</Label>
                        <Input
                            id="fingerprint"
                            name="fingerprint"
                            value={formData.fingerprint}
                            onChange={handleInputChange}
                            placeholder="8 character hex"
                            required
                        />
                    </div>

                    <div className="space-y-2">
                        <Label htmlFor="derivation_path">Derivation Path</Label>
                        <Input
                            id="derivation_path"
                            name="derivation_path"
                            value={formData.derivation_path}
                            onChange={handleInputChange}
                            placeholder="e.g., m/48'/1'/0'/2'"
                            required
                        />
                    </div>

                    {error && (
                        <div className="text-red-500 text-sm mt-2">
                            {error}
                        </div>
                    )}

                    <Button
                        type="submit"
                        className="w-full"
                        disabled={isSubmitting}
                    >
                        {isSubmitting ? 'Submitting...' : 'Submit Keys'}
                    </Button>
                </form>
            </CardContent>
        </Card>
    );
}