#!/bin/bash

set -euo pipefail

case "$1" in
    "anthropic" )
        # from https://docs.anthropic.com/en/api/messages-examples
        curl https://api.anthropic.com/v1/messages \
            --header "x-api-key: $ANTHROPIC_API_KEY" \
            --header "anthropic-version: 2023-06-01" \
            --header "content-type: application/json" \
            --data \
        '{
            "model": "claude-3-opus-20240229",
            "max_tokens": 1024,
            "messages": [
                {"role": "user", "content": "Hello, Claude"}
            ]
        }'
        ;;
    
    "anthropic-stream" )
        # from https://docs.anthropic.com/en/api/messages-examples
        curl https://api.anthropic.com/v1/messages \
            --header "x-api-key: $ANTHROPIC_API_KEY" \
            --header "anthropic-version: 2023-06-01" \
            --header "content-type: application/json" \
            --data \
        '{
            "model": "claude-3-opus-20240229",
            "max_tokens": 1024,
            "messages": [
                {"role": "user", "content": "Hello, Claude"}
            ],
            "stream": true
        }'
        ;;

    "openai" )
        # from https://platform.openai.com/docs/api-reference/chat
        curl https://api.openai.com/v1/chat/completions \
          -H "Content-Type: application/json" \
          -H "Authorization: Bearer $OPENAI_API_KEY" \
          -d '{
            "model": "No models available",
            "messages": [
              {
                "role": "system",
                "content": "You are a helpful assistant."
              },
              {
                "role": "user",
                "content": "Hello!"
              }
            ]
          }'
        ;;

    "openai-stream" )
        # from https://platform.openai.com/docs/api-reference/chat
        curl https://api.openai.com/v1/chat/completions \
          -H "Content-Type: application/json" \
          -H "Authorization: Bearer $OPENAI_API_KEY" \
          -d '{
            "model": "No models available",
            "messages": [
              {
                "role": "system",
                "content": "You are a helpful assistant."
              },
              {
                "role": "user",
                "content": "Hello!"
              }
            ],
            "stream": true
          }'
        ;;

esac