"use client";

import { useEffect } from "react";
import { useRouter } from "next/navigation";
import { useQuery, useMutation } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { Id } from "../../../convex/_generated/dataModel";
import { useUser } from "@/components/providers/user-provider";
import { Button } from "@/components/ui/button";
import { Badge } from "@/components/ui/badge";
import { Skeleton } from "@/components/ui/skeleton";
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { ArrowLeft, Users, Shield, Crown, UserMinus } from "lucide-react";

type UserRole = "bdfl" | "team" | "unset" | "admin" | "shepherd" | "member";

interface UserRecord {
  _id: Id<"users">;
  name: string;
  role: UserRole;
  avatarUrl?: string;
  boundaryEmail?: string;
  slackUserId?: string;
  isSpecialAccount?: boolean;
  createdAt: number;
}

// Get the equivalent new role for a legacy role (for sorting purposes)
function getNormalizedRole(role: string): "bdfl" | "team" | "unset" {
  if (role === "bdfl" || role === "admin") return "bdfl";
  if (role === "team" || role === "shepherd") return "team";
  return "unset";
}

function RoleBadge({ role }: { role: UserRole }) {
  const roleConfig = {
    bdfl: { label: "BDFL", variant: "bdfl" as const, icon: Crown },
    team: { label: "Team", variant: "team" as const, icon: Shield },
    unset: { label: "Unset", variant: "unset" as const, icon: UserMinus },
    // Legacy roles - show original name with "(legacy)" suffix
    admin: { label: "Admin (legacy)", variant: "admin" as const, icon: Crown },
    shepherd: { label: "Shepherd (legacy)", variant: "shepherd" as const, icon: Shield },
    member: { label: "Member (legacy)", variant: "member" as const, icon: UserMinus },
  };

  const config = roleConfig[role];
  const Icon = config.icon;

  return (
    <Badge variant={config.variant} className="gap-1">
      <Icon className="h-3 w-3" />
      {config.label}
    </Badge>
  );
}

function UserRow({
  user,
  currentUserId,
  currentUserRole,
  onRoleChange,
}: {
  user: UserRecord;
  currentUserId: Id<"users">;
  currentUserRole: UserRole;
  onRoleChange: (userId: Id<"users">, newRole: UserRole) => void;
}) {
  const isCurrentUser = user._id === currentUserId;
  // Check if current user has admin-level permissions (bdfl or legacy admin)
  const canEditBdfl = currentUserRole === "bdfl" || currentUserRole === "admin";
  // Can edit this user if we have admin permissions, or if the target user is not an admin
  const targetIsAdmin = user.role === "bdfl" || user.role === "admin";
  const canEditThisUser = canEditBdfl || !targetIsAdmin;

  return (
    <div className="flex items-center justify-between p-4 border rounded-lg bg-card">
      <div className="flex items-center gap-4">
        {user.avatarUrl ? (
          <img
            src={user.avatarUrl}
            alt={user.name}
            className="w-10 h-10 rounded-full"
          />
        ) : (
          <div className="w-10 h-10 rounded-full bg-muted flex items-center justify-center">
            <span className="text-muted-foreground font-medium">
              {user.name.charAt(0).toUpperCase()}
            </span>
          </div>
        )}
        <div>
          <div className="flex items-center gap-2">
            <span className="font-medium">{user.name}</span>
            {isCurrentUser && (
              <span className="text-xs text-muted-foreground">(you)</span>
            )}
          </div>
          {user.boundaryEmail && (
            <span className="text-sm text-muted-foreground">
              {user.boundaryEmail}
            </span>
          )}
          {user.slackUserId && (
            <span className="text-xs text-green-600 dark:text-green-400 ml-2">
              Slack connected
            </span>
          )}
        </div>
      </div>

      <div className="flex items-center gap-4">
        <RoleBadge role={user.role} />

        {canEditThisUser && !isCurrentUser && (
          <Select
            value={user.role}
            onValueChange={(value) => onRoleChange(user._id, value as UserRole)}
          >
            <SelectTrigger className="w-32">
              <SelectValue placeholder="Change role" />
            </SelectTrigger>
            <SelectContent>
              {canEditBdfl && <SelectItem value="bdfl">BDFL</SelectItem>}
              <SelectItem value="team">Team</SelectItem>
              <SelectItem value="unset">Unset</SelectItem>
            </SelectContent>
          </Select>
        )}
      </div>
    </div>
  );
}

export default function UsersPage() {
  const { user, userId, isLoading: userLoading, hasManagementPermissions } = useUser();
  const router = useRouter();

  const users = useQuery(
    api.users.listForManagement,
    userId && hasManagementPermissions ? { requesterId: userId } : "skip"
  );

  const updateRole = useMutation(api.users.updateRoleWithPermissions);

  useEffect(() => {
    if (!userLoading && !userId) {
      router.push("/login");
      return;
    }

    if (!userLoading && userId && !hasManagementPermissions) {
      router.push("/");
    }
  }, [userLoading, userId, hasManagementPermissions, router]);

  const handleRoleChange = async (targetUserId: Id<"users">, newRole: UserRole) => {
    if (!userId) return;

    try {
      await updateRole({
        requesterId: userId,
        targetUserId,
        role: newRole,
      });
    } catch (error) {
      console.error("Failed to update role:", error);
    }
  };

  if (userLoading || !hasManagementPermissions) {
    return (
      <div className="min-h-screen bg-background p-8">
        <div className="max-w-4xl mx-auto space-y-4">
          <Skeleton className="h-12 w-64" />
          <Skeleton className="h-8 w-96" />
          <div className="space-y-3">
            <Skeleton className="h-20 w-full" />
            <Skeleton className="h-20 w-full" />
            <Skeleton className="h-20 w-full" />
          </div>
        </div>
      </div>
    );
  }

  if (!user) {
    return null;
  }

  const sortedUsers = users
    ? [...users].sort((a, b) => {
        const roleOrder = { bdfl: 0, team: 1, unset: 2 };
        const aOrder = roleOrder[getNormalizedRole(a.role)];
        const bOrder = roleOrder[getNormalizedRole(b.role)];
        if (aOrder !== bOrder) {
          return aOrder - bOrder;
        }
        return a.name.localeCompare(b.name);
      })
    : [];

  // Group users by normalized role (legacy roles grouped with their modern equivalents)
  const usersByRole = {
    bdfl: sortedUsers.filter((u) => getNormalizedRole(u.role) === "bdfl"),
    team: sortedUsers.filter((u) => getNormalizedRole(u.role) === "team"),
    unset: sortedUsers.filter((u) => getNormalizedRole(u.role) === "unset"),
  };

  return (
    <div className="min-h-screen bg-background">
      <header className="border-b">
        <div className="max-w-4xl mx-auto px-4 py-4 flex items-center gap-4">
          <Button variant="ghost" size="sm" onClick={() => router.push("/")}>
            <ArrowLeft className="h-4 w-4 mr-2" />
            Back
          </Button>
          <div className="flex items-center gap-2">
            <Users className="h-5 w-5" />
            <h1 className="text-xl font-bold">User Management</h1>
          </div>
        </div>
      </header>

      <main className="max-w-4xl mx-auto px-4 py-8">
        <Card className="mb-8">
          <CardHeader>
            <CardTitle>Role Permissions</CardTitle>
            <CardDescription>
              Manage user roles to control access to features
            </CardDescription>
          </CardHeader>
          <CardContent>
            <div className="grid gap-4 md:grid-cols-3">
              <div className="p-4 border rounded-lg">
                <div className="flex items-center gap-2 mb-2">
                  <Crown className="h-4 w-4 text-amber-500" />
                  <span className="font-medium">BDFL</span>
                </div>
                <p className="text-sm text-muted-foreground">
                  Full administrative access. Can manage all users and assign any role.
                </p>
              </div>
              <div className="p-4 border rounded-lg">
                <div className="flex items-center gap-2 mb-2">
                  <Shield className="h-4 w-4 text-indigo-500" />
                  <span className="font-medium">Team</span>
                </div>
                <p className="text-sm text-muted-foreground">
                  Can view user management and assign Team or Unset roles.
                </p>
              </div>
              <div className="p-4 border rounded-lg">
                <div className="flex items-center gap-2 mb-2">
                  <UserMinus className="h-4 w-4 text-gray-400" />
                  <span className="font-medium">Unset</span>
                </div>
                <p className="text-sm text-muted-foreground">
                  Default role. No special permissions.
                </p>
              </div>
            </div>
          </CardContent>
        </Card>

        {users === undefined ? (
          <div className="space-y-3">
            <Skeleton className="h-20 w-full" />
            <Skeleton className="h-20 w-full" />
            <Skeleton className="h-20 w-full" />
          </div>
        ) : (
          <div className="space-y-8">
            {usersByRole.bdfl.length > 0 && (
              <section>
                <h2 className="text-lg font-semibold mb-4 flex items-center gap-2">
                  <Crown className="h-5 w-5 text-amber-500" />
                  BDFL ({usersByRole.bdfl.length})
                </h2>
                <div className="space-y-3">
                  {usersByRole.bdfl.map((u) => (
                    <UserRow
                      key={u._id}
                      user={u as UserRecord}
                      currentUserId={userId!}
                      currentUserRole={user.role}
                      onRoleChange={handleRoleChange}
                    />
                  ))}
                </div>
              </section>
            )}

            {usersByRole.team.length > 0 && (
              <section>
                <h2 className="text-lg font-semibold mb-4 flex items-center gap-2">
                  <Shield className="h-5 w-5 text-indigo-500" />
                  Team ({usersByRole.team.length})
                </h2>
                <div className="space-y-3">
                  {usersByRole.team.map((u) => (
                    <UserRow
                      key={u._id}
                      user={u as UserRecord}
                      currentUserId={userId!}
                      currentUserRole={user.role}
                      onRoleChange={handleRoleChange}
                    />
                  ))}
                </div>
              </section>
            )}

            {usersByRole.unset.length > 0 && (
              <section>
                <h2 className="text-lg font-semibold mb-4 flex items-center gap-2">
                  <UserMinus className="h-5 w-5 text-gray-400" />
                  Unset ({usersByRole.unset.length})
                </h2>
                <div className="space-y-3">
                  {usersByRole.unset.map((u) => (
                    <UserRow
                      key={u._id}
                      user={u as UserRecord}
                      currentUserId={userId!}
                      currentUserRole={user.role}
                      onRoleChange={handleRoleChange}
                    />
                  ))}
                </div>
              </section>
            )}

            {sortedUsers.length === 0 && (
              <div className="text-center py-12 text-muted-foreground">
                No users found
              </div>
            )}
          </div>
        )}
      </main>
    </div>
  );
}
