import { z } from 'zod';

import { declarationPageKindSchema } from '@/lib/generated-content/schemas';

export const exportedTypeDisplaySchema = z
  .object({ display: z.string().min(1) })
  .passthrough();

export const exportedSourceSchema = z
  .object({
    end: z.number().int(),
    file: z.string().min(1),
    start: z.number().int(),
  })
  .passthrough();

export const exportedParameterSchema = z
  .object({
    name: z.string().min(1),
    optional: z.boolean().optional(),
    ty: exportedTypeDisplaySchema,
  })
  .passthrough();

const exportedGenericSchema = z
  .object({
    bounds: z.array(z.string().min(1)),
    name: z.string().min(1),
  })
  .passthrough();

export const exportedSignatureSchema = z
  .object({
    generics: z.array(exportedGenericSchema).optional(),
    params: z.array(exportedParameterSchema),
    returns: exportedTypeDisplaySchema,
    throws: exportedTypeDisplaySchema.optional(),
  })
  .passthrough();

export const exportedMemberSchema = z
  .object({
    default: exportedTypeDisplaySchema.optional(),
    docstring: z.string().optional(),
    id: z.string().min(1),
    name: z.string().min(1),
    signature: exportedSignatureSchema.optional(),
    ty: exportedTypeDisplaySchema.optional(),
  })
  .passthrough();

export const exportedItemSchema = z
  .object({
    assoc_types: z.array(exportedMemberSchema).optional(),
    default_methods: z.array(exportedMemberSchema).optional(),
    detail: z.string().optional(),
    docstring: z.string().optional(),
    fields: z.array(exportedMemberSchema).optional(),
    generics: z.array(exportedGenericSchema).optional(),
    id: z.string().min(1),
    impls: z.array(z.string().min(1)).optional(),
    kind: declarationPageKindSchema,
    methods: z.array(exportedMemberSchema).optional(),
    name: z.string().min(1),
    namespace: z.array(z.string().min(1)).optional(),
    required_methods: z.array(exportedMemberSchema).optional(),
    resolved: exportedTypeDisplaySchema.optional(),
    signature: exportedSignatureSchema.optional(),
    source: exportedSourceSchema.optional(),
    variants: z.array(exportedMemberSchema).optional(),
  })
  .passthrough();

export const exportedImplementationSchema = z
  .object({
    assoc_bindings: z
      .array(
        z
          .object({
            name: z.string().min(1),
            ty: exportedTypeDisplaySchema,
          })
          .passthrough(),
      )
      .optional(),
    docstring: z.string().optional(),
    for_ty: exportedTypeDisplaySchema.optional(),
    id: z.string().min(1),
    interface: z.string().optional(),
    methods: z.array(exportedMemberSchema).optional(),
    source: exportedSourceSchema.optional(),
  })
  .passthrough();

export type ExportedImplementation = z.output<
  typeof exportedImplementationSchema
>;
export type ExportedGeneric = z.output<typeof exportedGenericSchema>;
export type ExportedItem = z.output<typeof exportedItemSchema>;
export type ExportedMember = z.output<typeof exportedMemberSchema>;
export type ExportedParameter = z.output<typeof exportedParameterSchema>;
export type ExportedSignature = z.output<typeof exportedSignatureSchema>;
export type ExportedSource = z.output<typeof exportedSourceSchema>;
