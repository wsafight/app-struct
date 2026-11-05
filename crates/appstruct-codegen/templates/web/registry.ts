import type { ComponentType, ReactNode } from "react";
import type { ResourceDefinition } from "../resource";

export interface FieldComponentProps {
  id: string;
  label: string;
  required: boolean;
  value: string | boolean | undefined;
  error?: string;
  readOnly: boolean;
  onChange(value: string | boolean): void;
}

export interface PageComponentProps {
  resources: readonly ResourceDefinition[];
}

export type PageScope = "public" | "account" | "workspace";

export interface PageLayoutProps extends PageComponentProps {
  scope: PageScope;
  pages: readonly CustomPageDefinition[];
  children: ReactNode;
}

export interface AppStructRegistry {
  fields: __FIELD_REGISTRY__;
  pages: __PAGE_REGISTRY__;
  layouts?: Partial<Record<PageScope, ComponentType<PageLayoutProps>>>;
}

export interface CustomPageDefinition {
  name: string;
  label: string;
  path: string;
  component: keyof AppStructRegistry["pages"];
  scope: PageScope;
}

export function defineAppStructRegistry<T extends AppStructRegistry>(
  registry: T,
): T {
  return registry;
}

export const customPages: readonly CustomPageDefinition[] = [__PAGES__];

export const workspacePath = "__WORKSPACE_PATH__";
