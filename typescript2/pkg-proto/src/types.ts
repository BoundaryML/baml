import type { InboundValue } from './generated/baml/cffi/v1/baml_inbound';

export type BamlJsValue<T = unknown> =
  | string
  | number
  | boolean
  | null
  | BamlJsValue<T>[]
  | BamlJsMap<T>
  | BamlJsHandle<T>
  | BamlJsClass<T>
  | BamlJsMedia
  | BamlJsPromptAst;

export type BamlJsMap<T = unknown> = { [key: string]: BamlJsValue<T> };
export type BamlJsHandle<T> = { $baml: { type: '$handle' }; handle: T };
export type BamlJsClass<T = unknown> = { $baml: { type: string } } & BamlJsMap<T>;

export type BamlJsMedia = {
  $baml: { type: '$media' };
  media_type: 'image' | 'audio' | 'pdf' | 'video' | 'other';
  mime_type?: string;
} & (
  | { content_type: 'url'; url: string }
  | { content_type: 'base64'; base64: string }
  | { content_type: 'file'; file: string }
);

export type BamlJsPromptAstSimple = { $baml: { type: '$prompt_ast_simple' } } & (
  | { content_type: 'string'; value: string }
  | { content_type: 'media'; value: BamlJsMedia }
  | { content_type: 'multiple'; value: BamlJsPromptAstSimple[] }
);

export type BamlJsPromptAstMessage = {
  $baml: { type: '$prompt_ast_message' };
  role: string;
  content: BamlJsPromptAstSimple | null;
  metadata?: unknown;
};

export type BamlJsPromptAst = {
  $baml: { type: '$prompt_ast' };
} & (
  | { content_type: 'simple'; value: BamlJsPromptAstSimple }
  | { content_type: 'message'; value: BamlJsPromptAstMessage }
  | { content_type: 'multiple'; value: BamlJsPromptAst[] }
);

/** Implemented by objects that need custom BAML serialization. */
export interface BamlSerializable {
  toBaml(): InboundValue;
}
