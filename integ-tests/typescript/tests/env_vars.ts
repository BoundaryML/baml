import { resetBamlEnvVars, traceAsync, traceSync } from "../baml_client"
import { b } from "./test-setup"

describe('Env Vars Tests', () => {
    it('should reset environment variables correctly', async () => {
        const envVars = {
            OPENAI_API_KEY: 'sk-1234567890',
    }
    resetBamlEnvVars(envVars)

    const topLevelSyncTracing = traceSync('name', () => {
      resetBamlEnvVars(envVars)
    })

    const atopLevelAsyncTracing = traceAsync('name', async () => {
      resetBamlEnvVars(envVars)
    })

    await expect(async () => {
      topLevelSyncTracing()
    }).rejects.toThrow('BamlError')

    await expect(async () => {
      await atopLevelAsyncTracing()
    }).rejects.toThrow('BamlError')

    await expect(async () => {
      await b.ExtractPeople(
        "My name is Harrison. My hair is black and I'm 6 feet tall. I'm pretty good around the hoop.",
      )
    }).rejects.toThrow('BamlClientHttpError')

    resetBamlEnvVars(
      Object.fromEntries(Object.entries(process.env).filter(([_, v]) => v !== undefined)) as Record<string, string>,
    )
    const people = await b.ExtractPeople(
      "My name is Harrison. My hair is black and I'm 6 feet tall. I'm pretty good around the hoop.",
        )
        expect(people.length).toBeGreaterThan(0)
    })
})
