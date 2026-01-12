"use client";

import { useState } from "react";
import { useRouter } from "next/navigation";
import { useUser } from "@/components/providers/user-provider";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";

export default function LoginPage() {
  const [name, setName] = useState("");
  const [passkey, setPasskey] = useState("");
  const [error, setError] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const { login } = useUser();
  const router = useRouter();

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!name.trim() || !passkey.trim()) return;

    setError("");
    setIsSubmitting(true);
    try {
      await login(name.trim(), passkey.trim());
      router.push("/");
    } catch (error) {
      if (error instanceof Error && error.message.includes("Invalid passkey")) {
        setError("Invalid passkey");
      } else {
        setError("Failed to login. Please try again.");
      }
      console.error("Failed to login:", error);
    } finally {
      setIsSubmitting(false);
    }
  };

  return (
    <div className="min-h-screen flex items-center justify-center bg-background p-4">
      <Card className="w-full max-w-md">
        <CardHeader className="text-center">
          <CardTitle className="text-2xl">Welcome to BEP Feedback</CardTitle>
          <CardDescription>
            Enter your name to start participating in BAML Enhancement Proposal
            discussions
          </CardDescription>
        </CardHeader>
        <CardContent>
          <form onSubmit={handleSubmit} className="space-y-4">
            <div className="space-y-2">
              <Label htmlFor="name">Your Name</Label>
              <Input
                id="name"
                type="text"
                placeholder="Enter your name"
                value={name}
                onChange={(e) => setName(e.target.value)}
                disabled={isSubmitting}
                autoFocus
              />
            </div>
            <div className="space-y-2">
              <Label htmlFor="passkey">Passkey</Label>
              <Input
                id="passkey"
                type="password"
                placeholder="Enter passkey"
                value={passkey}
                onChange={(e) => setPasskey(e.target.value)}
                disabled={isSubmitting}
              />
            </div>
            {error && (
              <p className="text-sm text-destructive">{error}</p>
            )}
            <Button
              type="submit"
              className="w-full"
              disabled={!name.trim() || !passkey.trim() || isSubmitting}
            >
              {isSubmitting ? "Entering..." : "Continue"}
            </Button>
          </form>
        </CardContent>
      </Card>
    </div>
  );
}
