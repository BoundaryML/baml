"use client";

import { useRouter } from "next/navigation";
import Link from "next/link";
import { useUser } from "@/components/providers/user-provider";
import { BepList } from "@/components/bep/bep-list";
import { BepCreateModal } from "@/components/bep/bep-create-modal";
import { BepExportAllDialog } from "@/components/bep/bep-export-all-dialog";
import { Button } from "@/components/ui/button";
import { Skeleton } from "@/components/ui/skeleton";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu";
import { LogOut, User, Users, ChevronDown, LogIn } from "lucide-react";

export default function Home() {
  const { user, userId, isLoading, logout, hasManagementPermissions } = useUser();
  const router = useRouter();

  const handleLogout = () => {
    logout();
    router.push("/login");
  };

  if (isLoading) {
    return (
      <div className="min-h-screen bg-background p-8">
        <div className="max-w-4xl mx-auto space-y-4">
          <Skeleton className="h-12 w-64" />
          <Skeleton className="h-8 w-96" />
          <div className="space-y-3">
            <Skeleton className="h-24 w-full" />
            <Skeleton className="h-24 w-full" />
            <Skeleton className="h-24 w-full" />
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="min-h-screen bg-background">
      <header className="border-b">
        <div className="max-w-[1600px] mx-auto px-4 py-4 flex items-center justify-between">
          <h1 className="text-2xl font-bold">BAML Enhancement Proposals</h1>
          <div className="flex items-center gap-4">
            {user ? (
              <DropdownMenu>
                <DropdownMenuTrigger asChild>
                  <Button variant="ghost" size="sm" className="gap-2">
                    {user.avatarUrl ? (
                      <img
                        src={user.avatarUrl}
                        alt={user.name}
                        className="w-6 h-6 rounded-full"
                      />
                    ) : (
                      <div className="w-6 h-6 rounded-full bg-muted flex items-center justify-center">
                        <span className="text-xs text-muted-foreground font-medium">
                          {user.name.charAt(0).toUpperCase()}
                        </span>
                      </div>
                    )}
                    <span className="text-sm">{user.name}</span>
                    <ChevronDown className="h-4 w-4 text-muted-foreground" />
                  </Button>
                </DropdownMenuTrigger>
                <DropdownMenuContent align="end" className="w-48">
                  <DropdownMenuItem onClick={() => router.push("/profile")}>
                    <User className="h-4 w-4 mr-2" />
                    Your Profile
                  </DropdownMenuItem>
                  {hasManagementPermissions && (
                    <DropdownMenuItem onClick={() => router.push("/users")}>
                      <Users className="h-4 w-4 mr-2" />
                      Manage Users
                    </DropdownMenuItem>
                  )}
                  <DropdownMenuSeparator />
                  <DropdownMenuItem onClick={handleLogout}>
                    <LogOut className="h-4 w-4 mr-2" />
                    Sign Out
                  </DropdownMenuItem>
                </DropdownMenuContent>
              </DropdownMenu>
            ) : (
              <Link href="/login">
                <Button variant="outline" size="sm" className="gap-2">
                  <LogIn className="h-4 w-4" />
                  Sign In
                </Button>
              </Link>
            )}
          </div>
        </div>
      </header>
      <main className="max-w-[1600px] mx-auto px-4 py-8">
        <div className="flex items-center justify-between mb-6">
          <h2 className="text-xl font-semibold">All Proposals</h2>
          <div className="flex items-center gap-2">
            <BepExportAllDialog />
            {userId && <BepCreateModal userId={userId} />}
          </div>
        </div>
        <BepList />
      </main>
    </div>
  );
}
