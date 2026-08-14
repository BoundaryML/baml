"use client";

import { useEffect, useState } from "react";
import { useRouter } from "next/navigation";
import { useMutation } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { useUser } from "@/components/providers/user-provider";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Skeleton } from "@/components/ui/skeleton";
import { Input } from "@/components/ui/input";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  ArrowLeft,
  User,
  Mail,
  MessageSquare,
  Shield,
  Crown,
  UserMinus,
  CheckCircle,
  XCircle,
  Key,
  Copy,
  RefreshCw,
  Eye,
  EyeOff,
} from "lucide-react";
import { Id } from "../../../convex/_generated/dataModel";

type UserRole = "bdfl" | "team" | "unset" | "admin" | "shepherd" | "member";

// Check if role is a legacy role that needs migration
function isLegacyRole(role: string): boolean {
  return role === "admin" || role === "shepherd" || role === "member";
}

// Check if user can have API tokens (team members only)
function canHaveApiToken(user: { role: string; isSpecialAccount?: boolean }): boolean {
  const hasTeamPermissions = ["bdfl", "team", "admin", "shepherd"].includes(user.role);
  return user.isSpecialAccount === true || hasTeamPermissions;
}

function RoleBadge({ role }: { role: UserRole }) {
  const roleConfig = {
    bdfl: { label: "BDFL", variant: "bdfl" as const, icon: Crown, description: "Full administrative access" },
    team: { label: "Team", variant: "team" as const, icon: Shield, description: "Team member with management access" },
    unset: { label: "Unset", variant: "unset" as const, icon: UserMinus, description: "No special permissions" },
    // Legacy roles
    admin: { label: "Admin (legacy)", variant: "admin" as const, icon: Crown, description: "Full administrative access (legacy role)" },
    shepherd: { label: "Shepherd (legacy)", variant: "shepherd" as const, icon: Shield, description: "Team member with management access (legacy role)" },
    member: { label: "Member (legacy)", variant: "member" as const, icon: UserMinus, description: "No special permissions (legacy role)" },
  };

  const config = roleConfig[role];
  const Icon = config.icon;

  return (
    <div className="flex items-center gap-2">
      <Badge variant={config.variant} className="gap-1">
        <Icon className="h-3 w-3" />
        {config.label}
      </Badge>
      <span className="text-sm text-muted-foreground">
        {config.description}
      </span>
    </div>
  );
}

export default function ProfilePage() {
  const { user, userId, isLoading } = useUser();
  const router = useRouter();
  const [apiToken, setApiToken] = useState<string | null>(null);
  const [showToken, setShowToken] = useState(false);
  const [isLoadingToken, setIsLoadingToken] = useState(false);
  const [copied, setCopied] = useState(false);

  const getOrCreateToken = useMutation(api.users.getOrCreateApiToken);
  const regenerateToken = useMutation(api.users.regenerateApiToken);

  useEffect(() => {
    if (!isLoading && !userId) {
      router.push("/login");
    }
  }, [isLoading, userId, router]);

  const handleGetToken = async () => {
    if (!userId) return;
    setIsLoadingToken(true);
    try {
      const result = await getOrCreateToken({ userId: userId as Id<"users"> });
      setApiToken(result.token);
      setShowToken(true);
    } catch (err) {
      console.error("Failed to get token:", err);
    } finally {
      setIsLoadingToken(false);
    }
  };

  const handleRegenerateToken = async () => {
    if (!userId) return;
    if (!confirm("This will invalidate your current token. Any agents using the old token will stop working. Continue?")) {
      return;
    }
    setIsLoadingToken(true);
    try {
      const result = await regenerateToken({ userId: userId as Id<"users"> });
      setApiToken(result.token);
      setShowToken(true);
    } catch (err) {
      console.error("Failed to regenerate token:", err);
    } finally {
      setIsLoadingToken(false);
    }
  };

  const handleCopyToken = async () => {
    if (!apiToken) return;
    try {
      await navigator.clipboard.writeText(apiToken);
      setCopied(true);
      setTimeout(() => setCopied(false), 2000);
    } catch (err) {
      console.error("Failed to copy:", err);
    }
  };

  if (isLoading) {
    return (
      <div className="min-h-screen bg-background p-8">
        <div className="max-w-2xl mx-auto space-y-4">
          <Skeleton className="h-12 w-64" />
          <Skeleton className="h-64 w-full" />
        </div>
      </div>
    );
  }

  if (!user) {
    return null;
  }

  return (
    <div className="min-h-screen bg-background">
      <header className="border-b">
        <div className="max-w-2xl mx-auto px-4 py-4 flex items-center gap-4">
          <Button variant="ghost" size="sm" onClick={() => router.push("/")}>
            <ArrowLeft className="h-4 w-4 mr-2" />
            Back
          </Button>
          <div className="flex items-center gap-2">
            <User className="h-5 w-5" />
            <h1 className="text-xl font-bold">Your Profile</h1>
          </div>
        </div>
      </header>

      <main className="max-w-2xl mx-auto px-4 py-8">
        <Card>
          <CardHeader>
            <div className="flex items-center gap-4">
              {user.avatarUrl ? (
                <img
                  src={user.avatarUrl}
                  alt={user.name}
                  className="w-16 h-16 rounded-full"
                />
              ) : (
                <div className="w-16 h-16 rounded-full bg-muted flex items-center justify-center">
                  <span className="text-2xl text-muted-foreground font-medium">
                    {user.name.charAt(0).toUpperCase()}
                  </span>
                </div>
              )}
              <div>
                <CardTitle className="text-2xl">{user.name}</CardTitle>
                <CardDescription>
                  Member since {new Date(user.createdAt).toLocaleDateString()}
                </CardDescription>
              </div>
            </div>
          </CardHeader>
          <CardContent className="space-y-6">
            <div className="space-y-4">
              <h3 className="text-lg font-semibold">Account Information</h3>

              <div className="grid gap-4">
                <div className="flex items-start gap-3 p-4 border rounded-lg">
                  <User className="h-5 w-5 text-muted-foreground mt-0.5" />
                  <div>
                    <div className="font-medium">Name</div>
                    <div className="text-muted-foreground">{user.name}</div>
                  </div>
                </div>

                <div className="flex items-start gap-3 p-4 border rounded-lg">
                  <Mail className="h-5 w-5 text-muted-foreground mt-0.5" />
                  <div className="flex-1">
                    <div className="font-medium">Email</div>
                    {user.boundaryEmail ? (
                      <div className="text-muted-foreground">{user.boundaryEmail}</div>
                    ) : (
                      <div className="text-muted-foreground italic">
                        No email connected
                      </div>
                    )}
                    {user.isSpecialAccount && (
                      <div className="text-xs text-green-600 dark:text-green-400 mt-1">
                        BoundaryML team member
                      </div>
                    )}
                  </div>
                </div>

                <div className="flex items-start gap-3 p-4 border rounded-lg">
                  <MessageSquare className="h-5 w-5 text-muted-foreground mt-0.5" />
                  <div className="flex-1">
                    <div className="font-medium">Slack Connection</div>
                    <div className="flex items-center gap-2 mt-1">
                      {user.slackUserId ? (
                        <>
                          <CheckCircle className="h-4 w-4 text-green-500" />
                          <span className="text-green-600 dark:text-green-400">
                            Connected
                          </span>
                        </>
                      ) : (
                        <>
                          <XCircle className="h-4 w-4 text-muted-foreground" />
                          <span className="text-muted-foreground">
                            Not connected
                          </span>
                        </>
                      )}
                    </div>
                    {user.slackUserId && (
                      <div className="text-xs text-muted-foreground mt-1">
                        You will be mentioned in Slack notifications for your comments and updates.
                      </div>
                    )}
                    {!user.slackUserId && user.boundaryEmail && (
                      <div className="text-xs text-muted-foreground mt-1">
                        Slack lookup may be pending. If you have a Slack account with your BoundaryML email, it will be linked automatically.
                      </div>
                    )}
                  </div>
                </div>
              </div>
            </div>

            {/* API Token Section - Only for team members */}
            {canHaveApiToken(user) && (
              <div className="space-y-4">
                <h3 className="text-lg font-semibold">API Access</h3>

                <div className="p-4 border rounded-lg space-y-4">
                  <div className="flex items-start gap-3">
                    <Key className="h-5 w-5 text-muted-foreground mt-0.5" />
                    <div className="flex-1">
                      <div className="font-medium">API Token</div>
                      <p className="text-sm text-muted-foreground mt-1">
                        Use this token to authenticate API requests from agents or scripts.
                      </p>
                    </div>
                  </div>

                  {apiToken ? (
                    <div className="space-y-3">
                      <div className="flex items-center gap-2">
                        <Input
                          type={showToken ? "text" : "password"}
                          value={apiToken}
                          readOnly
                          className="font-mono text-sm"
                        />
                        <Button
                          variant="outline"
                          size="icon"
                          onClick={() => setShowToken(!showToken)}
                          title={showToken ? "Hide token" : "Show token"}
                        >
                          {showToken ? (
                            <EyeOff className="h-4 w-4" />
                          ) : (
                            <Eye className="h-4 w-4" />
                          )}
                        </Button>
                        <Button
                          variant="outline"
                          size="icon"
                          onClick={handleCopyToken}
                          title="Copy token"
                        >
                          {copied ? (
                            <CheckCircle className="h-4 w-4 text-green-500" />
                          ) : (
                            <Copy className="h-4 w-4" />
                          )}
                        </Button>
                      </div>
                      <div className="flex items-center gap-2">
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={handleRegenerateToken}
                          disabled={isLoadingToken}
                        >
                          <RefreshCw className={`h-4 w-4 mr-2 ${isLoadingToken ? "animate-spin" : ""}`} />
                          Regenerate Token
                        </Button>
                        <span className="text-xs text-muted-foreground">
                          This will invalidate the current token
                        </span>
                      </div>
                    </div>
                  ) : (
                    <Button
                      onClick={handleGetToken}
                      disabled={isLoadingToken}
                    >
                      {isLoadingToken ? (
                        <>
                          <RefreshCw className="h-4 w-4 mr-2 animate-spin" />
                          Generating...
                        </>
                      ) : (
                        <>
                          <Key className="h-4 w-4 mr-2" />
                          Generate API Token
                        </>
                      )}
                    </Button>
                  )}

                  <div className="text-xs text-muted-foreground bg-muted p-3 rounded-md">
                    <p className="font-medium mb-1">Usage:</p>
                    <code className="block bg-background p-2 rounded text-xs overflow-x-auto">
                      curl -H &quot;Authorization: Bearer YOUR_TOKEN&quot; ...
                    </code>
                  </div>
                </div>
              </div>
            )}

            <div className="space-y-4">
              <h3 className="text-lg font-semibold">Role & Permissions</h3>

              <div className="p-4 border rounded-lg">
                <RoleBadge role={user.role} />

                {(user.role === "bdfl" || user.role === "admin") && (
                  <div className="mt-3 text-sm text-muted-foreground">
                    <p>As a {user.role === "bdfl" ? "BDFL" : "Admin"}, you have:</p>
                    <ul className="list-disc list-inside mt-2 space-y-1">
                      <li>Access to user management</li>
                      <li>Ability to assign any role to any user</li>
                      <li>Full administrative access</li>
                    </ul>
                    {isLegacyRole(user.role) && (
                      <p className="mt-2 text-amber-600 dark:text-amber-400">
                        Note: Your role will be migrated to BDFL automatically.
                      </p>
                    )}
                  </div>
                )}

                {(user.role === "team" || user.role === "shepherd") && (
                  <div className="mt-3 text-sm text-muted-foreground">
                    <p>As a {user.role === "team" ? "Team member" : "Shepherd"}, you have:</p>
                    <ul className="list-disc list-inside mt-2 space-y-1">
                      <li>Access to user management</li>
                      <li>Ability to assign Team or Unset roles</li>
                    </ul>
                    {isLegacyRole(user.role) && (
                      <p className="mt-2 text-amber-600 dark:text-amber-400">
                        Note: Your role will be migrated to Team automatically.
                      </p>
                    )}
                  </div>
                )}

                {(user.role === "unset" || user.role === "member") && (
                  <div className="mt-3 text-sm text-muted-foreground">
                    <p>You currently have no special permissions. Contact a Team member or BDFL to request access.</p>
                    {isLegacyRole(user.role) && (
                      <p className="mt-2 text-amber-600 dark:text-amber-400">
                        Note: Your role will be migrated to Unset automatically.
                      </p>
                    )}
                  </div>
                )}
              </div>
            </div>
          </CardContent>
        </Card>
      </main>
    </div>
  );
}
