// Type checking for client config options.

// Valid: all types correct.
client<llm> ValidOpenAI {
    provider openai
    options {
        model "gpt-4"
        max_tokens 100
        temperature 0.7
        base_url "https://api.openai.com/v1"
    }
}

// Invalid: max_tokens should be int, not string.
client<llm> BadMaxTokens {
    provider openai
    options {
        model "gpt-4"
        max_tokens "not a number"
    }
}

// Invalid: temperature should be float, not string.
client<llm> BadTemperature {
    provider openai
    options {
        model "gpt-4"
        temperature "warm"
    }
}

// Valid: Azure with resource_name + deployment_id.
client<llm> ValidAzure {
    provider azure-openai
    options {
        resource_name "my-resource"
        deployment_id "gpt-4o"
        api_version "2024-02-15-preview"
    }
}

// Valid: Azure with base_url.
client<llm> ValidAzureBaseUrl {
    provider azure-openai
    options {
        base_url "https://my-resource.openai.azure.com/openai/deployments/gpt-4o"
        api_version "2024-02-15-preview"
    }
}

// Invalid: Azure missing both base_url and resource_name + deployment_id.
client<llm> BadAzureMissingUrl {
    provider azure-openai
    options {
        api_version "2024-02-15-preview"
        model "gpt-4o"
    }
}
