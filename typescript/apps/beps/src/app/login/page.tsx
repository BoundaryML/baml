"use client";

import { useState, useEffect, Suspense } from "react";
import { useRouter, useSearchParams } from "next/navigation";
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

function GitHubIcon({ className }: { className?: string }) {
  return (
    <svg
      className={className}
      viewBox="0 0 24 24"
      fill="currentColor"
      xmlns="http://www.w3.org/2000/svg"
    >
      <path d="M12 0c-6.626 0-12 5.373-12 12 0 5.302 3.438 9.8 8.207 11.387.599.111.793-.261.793-.577v-2.234c-3.338.726-4.033-1.416-4.033-1.416-.546-1.387-1.333-1.756-1.333-1.756-1.089-.745.083-.729.083-.729 1.205.084 1.839 1.237 1.839 1.237 1.07 1.834 2.807 1.304 3.492.997.107-.775.418-1.305.762-1.604-2.665-.305-5.467-1.334-5.467-5.931 0-1.311.469-2.381 1.236-3.221-.124-.303-.535-1.524.117-3.176 0 0 1.008-.322 3.301 1.23.957-.266 1.983-.399 3.003-.404 1.02.005 2.047.138 3.006.404 2.291-1.552 3.297-1.23 3.297-1.23.653 1.653.242 2.874.118 3.176.77.84 1.235 1.911 1.235 3.221 0 4.609-2.807 5.624-5.479 5.921.43.372.823 1.102.823 2.222v3.293c0 .319.192.694.801.576 4.765-1.589 8.199-6.086 8.199-11.386 0-6.627-5.373-12-12-12z" />
    </svg>
  );
}

function LoginContent() {
  const [name, setName] = useState("");
  const [passkey, setPasskey] = useState("");
  const [error, setError] = useState("");
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [showPasskeyForm, setShowPasskeyForm] = useState(false);
  const { login, loginWithGitHub } = useUser();
  const router = useRouter();
  const searchParams = useSearchParams();

  useEffect(() => {
    const githubUserParam = searchParams.get("github_user");
    const errorParam = searchParams.get("error");

    if (errorParam) {
      const errorMessages: Record<string, string> = {
        no_code: "GitHub authentication failed. Please try again.",
        oauth_not_configured: "GitHub login is not configured.",
        failed_to_fetch_user: "Failed to fetch GitHub user info.",
        oauth_failed: "GitHub authentication failed. Please try again.",
        access_denied: "GitHub access was denied.",
      };
      setError(errorMessages[errorParam] || `Authentication error: ${errorParam}`);
      return;
    }

    if (githubUserParam) {
      try {
        const userData = JSON.parse(decodeURIComponent(githubUserParam));
        handleGitHubLogin(userData);
      } catch (e) {
        console.error("Failed to parse GitHub user data:", e);
        setError("Failed to process GitHub login.");
      }
    }
  }, [searchParams]);

  const handleGitHubLogin = async (userData: {
    githubId: string;
    name: string;
    avatarUrl?: string;
    boundaryEmail?: string;
  }) => {
    setIsSubmitting(true);
    setError("");
    try {
      await loginWithGitHub(userData);
      router.push("/");
    } catch (error) {
      console.error("GitHub login error:", error);
      setError("Failed to complete GitHub login. Please try again.");
    } finally {
      setIsSubmitting(false);
    }
  };

  const handlePasskeySubmit = async (e: React.FormEvent) => {
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

  const handleGitHubClick = () => {
    setIsSubmitting(true);
    window.location.href = "/api/auth/github";
  };

  return (
    <div className="min-h-screen flex items-center justify-center bg-background p-4">
      <Card className="w-full max-w-md">
        <CardHeader className="text-center">
          <CardTitle className="text-2xl">Welcome to BEP Feedback</CardTitle>
          <CardDescription>
            Sign in to participate in BAML Enhancement Proposal discussions
          </CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          {error && (
            <div className="p-3 rounded-md bg-destructive/10 text-destructive text-sm">
              {error}
            </div>
          )}

          <Button
            type="button"
            variant="outline"
            className="w-full"
            onClick={handleGitHubClick}
            disabled={isSubmitting}
          >
            <GitHubIcon className="w-5 h-5 mr-2" />
            {isSubmitting ? "Signing in..." : "Continue with GitHub"}
          </Button>

          <div className="relative">
            <div className="absolute inset-0 flex items-center">
              <span className="w-full border-t" />
            </div>
            <div className="relative flex justify-center text-xs uppercase">
              <span className="bg-background px-2 text-muted-foreground">
                Or
              </span>
            </div>
          </div>

          {!showPasskeyForm ? (
            <Button
              type="button"
              variant="ghost"
              className="w-full text-muted-foreground"
              onClick={() => setShowPasskeyForm(true)}
              disabled={isSubmitting}
            >
              Sign in with passkey
            </Button>
          ) : (
            <form onSubmit={handlePasskeySubmit} className="space-y-4">
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
              <div className="flex gap-2">
                <Button
                  type="button"
                  variant="ghost"
                  className="flex-1"
                  onClick={() => setShowPasskeyForm(false)}
                  disabled={isSubmitting}
                >
                  Back
                </Button>
                <Button
                  type="submit"
                  className="flex-1"
                  disabled={!name.trim() || !passkey.trim() || isSubmitting}
                >
                  {isSubmitting ? "Signing in..." : "Continue"}
                </Button>
              </div>
            </form>
          )}
        </CardContent>
      </Card>
    </div>
  );
}

export default function LoginPage() {
  return (
    <Suspense
      fallback={
        <div className="min-h-screen flex items-center justify-center bg-background p-4">
          <Card className="w-full max-w-md">
            <CardHeader className="text-center">
              <CardTitle className="text-2xl">Welcome to BEP Feedback</CardTitle>
              <CardDescription>Loading...</CardDescription>
            </CardHeader>
          </Card>
        </div>
      }
    >
      <LoginContent />
    </Suspense>
  );
}
