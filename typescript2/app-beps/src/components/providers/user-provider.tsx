"use client";

import {
  createContext,
  useContext,
  useState,
  ReactNode,
  useSyncExternalStore,
  useCallback,
} from "react";
import { useMutation, useQuery } from "convex/react";
import { api } from "../../../convex/_generated/api";
import { Id } from "../../../convex/_generated/dataModel";

interface User {
  _id: Id<"users">;
  name: string;
  // Includes both new roles and legacy roles (for backwards compatibility)
  role: "bdfl" | "team" | "unset" | "admin" | "shepherd" | "member";
  avatarUrl?: string;
  boundaryEmail?: string;
  slackUserId?: string;
  isSpecialAccount?: boolean;
  createdAt: number;
}

interface GitHubUserData {
  githubId: string;
  name: string;
  avatarUrl?: string;
  boundaryEmail?: string;
}

interface UserContextType {
  user: User | null;
  userId: Id<"users"> | null;
  isLoading: boolean;
  hasManagementPermissions: boolean;
  login: (name: string, passkey: string) => Promise<void>;
  loginWithGitHub: (userData: GitHubUserData) => Promise<void>;
  logout: () => void;
}

const UserContext = createContext<UserContextType | undefined>(undefined);

const USER_ID_KEY = "bep-user-id";

// Custom hook for localStorage with SSR support
function useLocalStorageUserId() {
  const [userId, setUserId] = useState<Id<"users"> | null>(null);
  const [isHydrated, setIsHydrated] = useState(false);

  // Use useSyncExternalStore for safe hydration
  const subscribe = useCallback((callback: () => void) => {
    window.addEventListener("storage", callback);
    return () => window.removeEventListener("storage", callback);
  }, []);

  const getSnapshot = useCallback(() => {
    return localStorage.getItem(USER_ID_KEY);
  }, []);

  const getServerSnapshot = useCallback(() => {
    return null;
  }, []);

  const storedValue = useSyncExternalStore(
    subscribe,
    getSnapshot,
    getServerSnapshot
  );

  // Hydrate on mount
  if (typeof window !== "undefined" && !isHydrated) {
    setIsHydrated(true);
    const stored = localStorage.getItem(USER_ID_KEY);
    if (stored && stored !== userId) {
      setUserId(stored as Id<"users">);
    }
  }

  const setStoredUserId = useCallback((newId: Id<"users"> | null) => {
    if (newId) {
      localStorage.setItem(USER_ID_KEY, newId);
    } else {
      localStorage.removeItem(USER_ID_KEY);
    }
    setUserId(newId);
  }, []);

  return {
    userId: userId ?? (storedValue as Id<"users"> | null),
    setUserId: setStoredUserId,
    isHydrated,
  };
}

export function UserProvider({ children }: { children: ReactNode }) {
  const { userId: storedUserId, setUserId: setStoredUserId, isHydrated } = useLocalStorageUserId();

  const getOrCreate = useMutation(api.users.getOrCreate);
  const getOrCreateFromGitHub = useMutation(api.users.getOrCreateFromGitHub);
  const user = useQuery(
    api.users.get,
    storedUserId ? { id: storedUserId } : "skip"
  );

  const login = async (name: string, passkey: string) => {
    const userId = await getOrCreate({ name, passkey });
    setStoredUserId(userId);
  };

  const loginWithGitHub = async (userData: GitHubUserData) => {
    const userId = await getOrCreateFromGitHub({
      githubId: userData.githubId,
      name: userData.name,
      avatarUrl: userData.avatarUrl,
      boundaryEmail: userData.boundaryEmail,
    });
    setStoredUserId(userId);
  };

  const logout = () => {
    setStoredUserId(null);
  };

  // Check if user has management permissions (BDFL or Team, including legacy roles)
  const hasManagementPermissions = user ? (
    user.role === "bdfl" || user.role === "team" ||
    user.role === "admin" || user.role === "shepherd"
  ) : false;

  return (
    <UserContext.Provider
      value={{
        user: user ?? null,
        userId: storedUserId,
        isLoading: !isHydrated || (storedUserId !== null && user === undefined),
        hasManagementPermissions,
        login,
        loginWithGitHub,
        logout,
      }}
    >
      {children}
    </UserContext.Provider>
  );
}

export function useUser() {
  const context = useContext(UserContext);
  if (context === undefined) {
    throw new Error("useUser must be used within a UserProvider");
  }
  return context;
}
