// Machine registration page

import { useState, useEffect } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { CommandResult } from "../types/events";

export default function MachineRegistration() {
  const [machineName, setMachineName] = useState("");
  const [machineId, setMachineId] = useState("");
  const [machineType, setMachineType] = useState("");
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    // Get machine info from window context if available
    // This should be passed from the backend command
    const urlParams = new URLSearchParams(window.location.search);
    const idParam = urlParams.get("machine_id");
    const typeParam = urlParams.get("machine_type");

    if (idParam) setMachineId(idParam);
    if (typeParam) setMachineType(typeParam);
  }, []);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    setLoading(true);
    setError("");

    try {
      const result = await invoke<CommandResult>("cmd_register_new_machine", {
        machineId,
        machineName,
        machineType,
      });

      if (!result.success) {
        setError(result.message || "Registration failed");
      }
    } catch (err) {
      console.error("Registration failed:", err);
      setError("Failed to register machine. Please try again.");
    } finally {
      setLoading(false);
    }
  };

  return (
    <div className="page registration-page">
      <div className="registration-content">
        <h1>Machine Registration</h1>
        <p>Register this machine to continue</p>

        {error && <div className="error-message">{error}</div>}

        <form onSubmit={handleSubmit}>
          <div className="form-group">
            <label>Machine Name</label>
            <input
              type="text"
              placeholder="Enter a name for this machine"
              value={machineName}
              onChange={(e) => setMachineName(e.target.value)}
              disabled={loading}
              required
            />
          </div>

          {machineId && (
            <div className="form-group">
              <label>Machine ID</label>
              <input type="text" value={machineId} disabled />
            </div>
          )}

          {machineType && (
            <div className="form-group">
              <label>Machine Type</label>
              <input type="text" value={machineType} disabled />
            </div>
          )}

          <button type="submit" disabled={loading || !machineName}>
            {loading ? "Registering..." : "Register Machine"}
          </button>
        </form>
      </div>
    </div>
  );
}
