"use client"

import { useState } from "react"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Card, CardContent, CardFooter, CardHeader, CardTitle } from "@/components/ui/card"

export default function Component() {
  const [deviceName, setDeviceName] = useState("")
  const [xpub, setXpub] = useState("")
  const [derivationPath, setDerivationPath] = useState("")
  const [error, setError] = useState("")

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault()
    setError("")

    if (!deviceName || !xpub || !derivationPath) {
      setError("Please fill in all fields")
      return
    }

    // Here you would typically send the data to your backend or perform further processing
    console.log("Device setup data:", { deviceName, xpub, derivationPath })

    // Reset form after successful submission
    setDeviceName("")
    setXpub("")
    setDerivationPath("")

    alert("Device setup submitted successfully!")
  }

  return (
    <Card className="w-full max-w-md mx-auto">
      <CardHeader>
        <CardTitle>Device Setup</CardTitle>
      </CardHeader>
      <form onSubmit={handleSubmit}>
        <CardContent className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="deviceName">Device Name</Label>
            <Input
              id="deviceName"
              value={deviceName}
              onChange={(e) => setDeviceName(e.target.value)}
              placeholder="Enter device name"
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="xpub">Extended Public Key (xpub)</Label>
            <Input
              id="xpub"
              value={xpub}
              onChange={(e) => setXpub(e.target.value)}
              placeholder="Enter xpub"
            />
          </div>
          <div className="space-y-2">
            <Label htmlFor="derivationPath">Derivation Path</Label>
            <Input
              id="derivationPath"
              value={derivationPath}
              onChange={(e) => setDerivationPath(e.target.value)}
              placeholder="Enter derivation path"
            />
          </div>
        </CardContent>
        <CardFooter>
          <Button type="submit" className="w-full">Set Up Device</Button>
        </CardFooter>
      </form>
    </Card>
  )
}