import { AssumeRoleCommand, STSClient } from '@aws-sdk/client-sts'
import { ClientRegistry } from '@boundaryml/baml'
import { b } from '../test-setup'

describe('AWS Provider', () => {
  it('should support AWS', async () => {
    const res = await b.TestAws('Dr. Pepper')
    expect(res.length).toBeGreaterThan(0)
  })

  it('should handle invalid AWS region gracefully', async () => {
    await expect(async () => {
      await b.TestAwsInvalidRegion('Write a nice short story about Dr. Pepper')
    }).rejects.toMatchObject({
      code: 'GenericFailure',
    })
  })

  it('should handle invalid AWS access key gracefully', async () => {
    // Clear all AWS-related environment variables

    // Create a new client registry with no environment credentials
    const cr = new ClientRegistry()
    cr.addLlmClient('InvalidAwsClient', 'aws-bedrock', {
      model_id: 'meta.llama3-8b-instruct-v1:0',
      region: 'us-east-1',
      access_key_id: 'AKIAINVALID12345678',
      secret_access_key: 'abcdef1234567890abcdef1234567890abcdef12',
    })
    cr.setPrimary('InvalidAwsClient')

    await expect(async () => {
      await b.TestAwsInvalidAccessKey('Write a nice short story about Dr. Pepper', { clientRegistry: cr })
    }).rejects.toMatchObject({
      name: 'BamlClientHttpError',
      status_code: 403,
      client_name: 'InvalidAwsClient',
    })
  })

  describe('Streaming', () => {
    it('should support streaming in AWS', async () => {
      const stream = b.stream.TestAws('Dr. Pepper')
      const msgs: string[] = []
      for await (const msg of stream) {
        msgs.push(msg ?? '')
      }
      const final = await stream.getFinalResponse()

      expect(final.length).toBeGreaterThan(0)
      expect(msgs.length).toBeGreaterThan(0)
      for (let i = 0; i < msgs.length - 2; i++) {
        expect(msgs[i + 1].startsWith(msgs[i])).toBeTruthy()
      }
      expect(msgs.at(-1)).toEqual(final)
    })
  })

  describe('Dynamic Client Registry', () => {
    describe('Credential Resolution', () => {
      it('should handle session credentials correctly', async () => {
        const sts = new STSClient({
          region: 'us-east-1',
          credentials: {
            accessKeyId: process.env.AWS_ACCESS_KEY_ID ?? '',
            secretAccessKey: process.env.AWS_SECRET_ACCESS_KEY ?? '',
          },
        })
        const { Credentials } = await sts.send(
          new AssumeRoleCommand({
            RoleArn: 'arn:aws:iam::404337120808:role/bedrock-access-role-integ-tests',
            RoleSessionName: 'BamlTestSession',
            DurationSeconds: 900,
          }),
        )

        if (!Credentials) {
          throw new Error('Failed to get credentials from STS')
        }

        const cr = new ClientRegistry()
        cr.addLlmClient('DynamicAWSClient', 'aws-bedrock', {
          model_id: 'meta.llama3-8b-instruct-v1:0',
          region: 'us-east-1',
          access_key_id: Credentials.AccessKeyId,
          secret_access_key: Credentials.SecretAccessKey,
          session_token: Credentials.SessionToken,
        })
        cr.setPrimary('DynamicAWSClient')

        const result = await b.TestAws('Dr. Pepper', { clientRegistry: cr })
        expect(result.length).toBeGreaterThan(0)
      })

      it('should require region in all environments', async () => {
        // Clear all region-related environment variables
        const cr = new ClientRegistry()
        cr.addLlmClient('DynamicAWSClient', 'aws-bedrock', {
          model_id: 'meta.llama3-8b-instruct-v1:0',
          access_key_id: 'test',
          secret_access_key: 'test',
        })
        cr.setPrimary('DynamicAWSClient')

        try {
          await b.TestAws('Dr. Pepper', { clientRegistry: cr })
        } catch (e) {
          expect(e).toMatchObject({
            name: 'BamlClientHttpError',
            status_code: 403,
            client_name: 'DynamicAWSClient',
            message: expect.stringContaining('BamlError: BamlClientError: BamlClientHttpError:'),
          })
          return
        }
        throw new Error('Expected error was not thrown')
      })

      it('should throw error when region is empty or AWS_REGION is unset', async () => {
        // Clear all region-related environment variables

        const crEmptyRegion = new ClientRegistry()
        crEmptyRegion.addLlmClient('DynamicAWSClient', 'aws-bedrock', {
          model_id: 'meta.llama3-8b-instruct-v1:0',
          region: '',
          access_key_id: 'test',
          secret_access_key: 'test',
        })
        crEmptyRegion.setPrimary('DynamicAWSClient')

        await expect(async () => {
          await b.TestAws('Dr. Pepper', { clientRegistry: crEmptyRegion })
        }).rejects.toMatchObject({
          code: 'GenericFailure',
        })

        const crNoEnvRegion = new ClientRegistry()
        crNoEnvRegion.addLlmClient('DynamicAWSClient', 'aws-bedrock', {
          model_id: 'meta.llama3-8b-instruct-v1:0',
          access_key_id: 'test',
          secret_access_key: 'test',
        })
        crNoEnvRegion.setPrimary('DynamicAWSClient')

        await expect(async () => {
          await b.TestAws('Dr. Pepper', { clientRegistry: crNoEnvRegion })
        }).rejects.toMatchObject({
          name: 'BamlClientHttpError',
          status_code: 403,
          client_name: 'DynamicAWSClient',
          message: expect.stringContaining('BamlError: BamlClientError: BamlClientHttpError:'),
        })
      })
    })

    it('should support dynamic client configuration', async () => {
      const cr = new ClientRegistry()
      cr.addLlmClient('DynamicAWSClient', 'aws-bedrock', {
        model_id: 'meta.llama3-8b-instruct-v1:0',
        region: 'us-east-1',
        inference_configuration: {
          max_tokens: 100,
        },
      })
      cr.setPrimary('DynamicAWSClient')

      const result = await b.TestAws('Dr. Pepper', { clientRegistry: cr })
      expect(result.length).toBeGreaterThan(0)
    })

    it('should support AWS credentials configuration', async () => {
      const cr = new ClientRegistry()
      cr.addLlmClient('DynamicAWSClient', 'aws-bedrock', {
        model_id: 'meta.llama3-8b-instruct-v1:0',
        region: 'us-east-1',
        access_key_id: 'test-access-key',
        secret_access_key: 'test-secret-key',
      })
      cr.setPrimary('DynamicAWSClient')

      await expect(async () => {
        await b.TestAws('Dr. Pepper', { clientRegistry: cr })
      }).rejects.toMatchObject({
        name: 'BamlClientHttpError',
        status_code: 403,
        client_name: 'DynamicAWSClient',
        message: expect.stringContaining('BamlError: BamlClientError: BamlClientHttpError:'),
      })
    })

    it('should support AWS profile configuration', async () => {
      const cr = new ClientRegistry()
      cr.addLlmClient('DynamicAWSClient', 'aws-bedrock', {
        model_id: 'meta.llama3-8b-instruct-v1:0',
        region: 'us-east-1',
        profile: 'boundaryml-dev',
        inference_configuration: {
          max_tokens: 100,
        },
      })
      cr.setPrimary('DynamicAWSClient')

      const result = await b.TestAws('Dr. Pepper', { clientRegistry: cr })
      expect(result.length).toBeGreaterThan(0)
    })

    it('should support both model and model_id parameters', async () => {
      // Set AWS_PROFILE for this specific test
      // process.env.AWS_PROFILE = 'boundaryml-dev'

      // Test with model_id parameter
      const crWithModelId = new ClientRegistry()
      crWithModelId.addLlmClient('DynamicAWSClientModelId', 'aws-bedrock', {
        model_id: 'meta.llama3-8b-instruct-v1:0',
        region: 'us-east-1',
        inference_configuration: {
          max_tokens: 100,
        },
      })
      crWithModelId.setPrimary('DynamicAWSClientModelId')
      const resultWithModelId = await b.TestAws('Dr. Pepper', { clientRegistry: crWithModelId })
      expect(resultWithModelId.length).toBeGreaterThan(0)

      // Test with model parameter (legacy format)
      const crWithModel = new ClientRegistry()
      crWithModel.addLlmClient('DynamicAWSClientModel', 'aws-bedrock', {
        model: 'meta.llama3-8b-instruct-v1:0',
        region: 'us-east-1',
        inference_configuration: {
          max_tokens: 100,
        },
      })
      crWithModel.setPrimary('DynamicAWSClientModel')
      const resultWithModel = await b.TestAws('Dr. Pepper', { clientRegistry: crWithModel })
      expect(resultWithModel.length).toBeGreaterThan(0)
    })

    it('should handle invalid configuration gracefully', async () => {
      const cr = new ClientRegistry()
      cr.addLlmClient('DynamicAWSClient', 'aws-bedrock', {
        model_id: 'meta.llama3-8b-instruct-v1:0',
        region: 'invalid-region',
        inference_configuration: {
          max_tokens: 100,
        },
      })
      cr.setPrimary('DynamicAWSClient')

      await expect(async () => {
        await b.TestAws('Dr. Pepper', { clientRegistry: cr })
      }).rejects.toMatchObject({
        code: 'GenericFailure',
      })
    })

    it('should handle non-existent model gracefully', async () => {
      const cr = new ClientRegistry()
      cr.addLlmClient('DynamicAWSClient', 'aws-bedrock', {
        model_id: 'non-existent-model-123',
        region: 'us-east-1',
        inference_configuration: {
          max_tokens: 100,
        },
      })
      cr.setPrimary('DynamicAWSClient')

      await expect(async () => {
        await b.TestAws('Dr. Pepper', { clientRegistry: cr })
      }).rejects.toMatchObject({
        name: 'BamlClientHttpError',
        status_code: 401,
        client_name: 'DynamicAWSClient',
        message: expect.stringContaining('BamlError: BamlClientError: BamlClientHttpError:'),
      })
    })

    it('should throw error when using temporary credentials without session token', async () => {
      // Clear all AWS-related environment variables

      const sts = new STSClient({
        region: 'us-east-1',
        credentials: {
          accessKeyId: process.env.AWS_ACCESS_KEY_ID ?? '',
          secretAccessKey: process.env.AWS_SECRET_ACCESS_KEY ?? '',
        },
      })
      const { Credentials } = await sts.send(
        new AssumeRoleCommand({
          RoleArn: 'arn:aws:iam::404337120808:role/bedrock-access-role-integ-tests',
          RoleSessionName: 'BamlTestSession',
          DurationSeconds: 900,
        }),
      )

      if (!Credentials) {
        throw new Error('Failed to get credentials from STS')
      }

      const cr = new ClientRegistry()
      cr.addLlmClient('DynamicAWSClient', 'aws-bedrock', {
        model_id: 'meta.llama3-8b-instruct-v1:0',
        region: 'us-east-1',
        access_key_id: Credentials.AccessKeyId,
        secret_access_key: Credentials.SecretAccessKey,
        // Intentionally omit session_token
      })
      cr.setPrimary('DynamicAWSClient')

      await expect(async () => {
        await b.TestAwsInvalidSessionToken('Dr. Pepper', { clientRegistry: cr })
      }).rejects.toMatchObject({
        name: 'BamlClientHttpError',
        status_code: 403,
        client_name: 'DynamicAWSClient',
        message: expect.stringContaining('BamlError: BamlClientError: BamlClientHttpError:'),
      })
    })

    it('should throw error when region is not provided', async () => {
      const cr = new ClientRegistry()
      cr.addLlmClient('DynamicAWSClient', 'aws-bedrock', {
        model_id: 'meta.llama3-8b-instruct-v1:0',
        region: null,
      })
      cr.setPrimary('DynamicAWSClient')

      await expect(async () => {
        await b.TestAws('Dr. Pepper', { clientRegistry: cr })
      }).rejects.toMatchObject({
        code: 'GenericFailure',
      })
    })

    it('should throw error when using invalid profile', async () => {
      // Clear any existing profile
      const cr = new ClientRegistry()
      cr.addLlmClient('DynamicAWSClient', 'aws-bedrock', {
        model_id: 'meta.llama3-8b-instruct-v1:0',
        region: 'us-east-1',
        access_key_id: null,
        secret_access_key: null,
        profile: 'non-existent-profile-123',
      })
      cr.setPrimary('DynamicAWSClient')

      await expect(async () => {
        await b.TestAws('Dr. Pepper', { clientRegistry: cr })
      }).rejects.toMatchObject({
        code: 'GenericFailure',
      })
    })

    it('should support both AWS_REGION and AWS_DEFAULT_REGION environment variables', async () => {
      const crWithAwsRegion = new ClientRegistry()
      crWithAwsRegion.addLlmClient('DynamicAWSClient', 'aws-bedrock', {
        model_id: 'meta.llama3-8b-instruct-v1:0',
        // Don't specify region, let it use AWS_REGION
        inference_configuration: {
          max_tokens: 100,
        },
      })
      crWithAwsRegion.setPrimary('DynamicAWSClient')

      const resultWithAwsRegion = await b.TestAws('Dr. Pepper', { clientRegistry: crWithAwsRegion })
      expect(resultWithAwsRegion.length).toBeGreaterThan(0)

      const crWithDefaultRegion = new ClientRegistry()
      crWithDefaultRegion.addLlmClient('DynamicAWSClient', 'aws-bedrock', {
        model_id: 'meta.llama3-8b-instruct-v1:0',
        // Don't specify region, let it use AWS_DEFAULT_REGION
        inference_configuration: {
          max_tokens: 100,
        },
      })
      crWithDefaultRegion.setPrimary('DynamicAWSClient')

      const resultWithDefaultRegion = await b.TestAws('Dr. Pepper', { clientRegistry: crWithDefaultRegion })
      expect(resultWithDefaultRegion.length).toBeGreaterThan(0)

      // Test that AWS_REGION takes precedence over AWS_DEFAULT_REGION
      process.env.AWS_REGION = 'us-east-1'
      process.env.AWS_DEFAULT_REGION = 'us-west-2' // Different region

      const crWithBothRegions = new ClientRegistry()
      crWithBothRegions.addLlmClient('DynamicAWSClient', 'aws-bedrock', {
        model_id: 'meta.llama3-8b-instruct-v1:0',
        // Don't specify region, should use AWS_REGION over AWS_DEFAULT_REGION
        inference_configuration: {
          max_tokens: 100,
        },
      })
      crWithBothRegions.setPrimary('DynamicAWSClient')

      const resultWithBothRegions = await b.TestAws('Dr. Pepper', { clientRegistry: crWithBothRegions })
      expect(resultWithBothRegions.length).toBeGreaterThan(0)
    })
  })
})
