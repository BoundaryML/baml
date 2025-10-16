# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## TypeScript Workspace Overview

This is a pnpm workspace within the BAML monorepo, containing TypeScript/JavaScript packages and applications. The workspace is managed by Turbo for build orchestration.

### Tooling

- Always use `pnpm` to manipulate dependencies (never add a dependency to package.json manually)
- Next.js is our primary backend framework
- ShadCN components are stored in packages/ui/
- Frontend code generally uses Jotai atoms for sharing state between React components
