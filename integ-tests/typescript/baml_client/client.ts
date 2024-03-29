// This file is auto-generated. Do not edit this file manually.
//
// Disable formatting for this file to avoid linting errors.
// tslint:disable
// @ts-nocheck
/* eslint-disable */


import { clientManager } from '@boundaryml/baml-core/client_manager';


import { loadEnvVars } from '@boundaryml/baml-core';
    loadEnvVars();

const Claude = clientManager.createClient('Claude', 'baml-anthropic-chat', {
    model: "claude-3-haiku-20240307",
    api_key: process.env.ANTHROPIC_API_KEY
});

const GPT35 = clientManager.createClient('GPT35', 'baml-openai-chat', {
    model: "gpt-3.5-turbo",
    api_key: process.env.OPENAI_API_KEY
});

const GPT4 = clientManager.createClient('GPT4', 'baml-openai-chat', {
    model: "gpt-4",
    api_key: process.env.OPENAI_API_KEY
});

const GPT4Turbo = clientManager.createClient('GPT4Turbo', 'baml-openai-chat', {
    model: "gpt-4-1106-preview",
    api_key: process.env.OPENAI_API_KEY
});

const Lottery_ComplexSyntax = clientManager.createClient('Lottery_ComplexSyntax', 'baml-round-robin', {
    start: 0,
    strategy: [{ "client": "GPT35" }, { "client": "Claude" }]
});

const Lottery_SimpleSyntax = clientManager.createClient('Lottery_SimpleSyntax', 'baml-round-robin', {
    start: 0,
    strategy: ["GPT35", "Claude"]
});

const Resilient_ComplexSyntax = clientManager.createClient('Resilient_ComplexSyntax', 'baml-fallback', {
    strategy: [{ "client": "GPT4Turbo" }, { "client": "GPT35" }, { "client": "Claude" }]
});

const Resilient_SimpleSyntax = clientManager.createClient('Resilient_SimpleSyntax', 'baml-fallback', {
    strategy: ["GPT4Turbo", "GPT35", "Claude"]
});


export { Claude, GPT35, GPT4, GPT4Turbo, Lottery_ComplexSyntax, Lottery_SimpleSyntax, Resilient_ComplexSyntax, Resilient_SimpleSyntax }

