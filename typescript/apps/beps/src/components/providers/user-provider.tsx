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
  role: "admin" | "shepherd" | "member";
  avatarUrl?: string;
  createdAt: number;
}

interface UserContextType {
  user: User | null;
  userId: Id<"users"> | null;
  isLoading: boolean;
  login: (name: string, passkey: string) => Promise<void>;
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
  const user = useQuery(
    api.users.get,
    storedUserId ? { id: storedUserId } : "skip"
  );

  const login = async (name: string, passkey: string) => {
    const userId = await getOrCreate({ name, passkey });
    setStoredUserId(userId);
  };

  const logout = () => {
    setStoredUserId(null);
  };

  return (
    <UserContext.Provider
      value={{
        user: user ?? null,
        userId: storedUserId,
        isLoading: !isHydrated || (storedUserId !== null && user === undefined),
        login,
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
