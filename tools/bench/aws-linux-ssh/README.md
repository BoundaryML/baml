# AWS Linux SSH benchmark hosts

This directory provisions two directly reachable Amazon Linux 2023 EC2 hosts for architecture-specific micro-GC benchmarks:

| Host | EC2 type | Architecture |
| --- | --- | --- |
| `sam-microgc-test-x64` | `t2.micro` | x86_64 |
| `sam-microgc-test-graviton` | `t4g.micro` | arm64 / Graviton |

There is no Graviton instance in the T2 family, so the Arm host uses the closest micro-sized burstable Graviton type, `t4g.micro`. This is useful for architecture-specific testing but is not an architecture-only comparison: T2 and T4g are different EC2 generations. Both instances use standard CPU credits so sustained load is throttled after credits are exhausted rather than incurring unlimited-mode surplus-credit charges.

The CloudFormation stack creates a dedicated VPC, public subnet, internet gateway, SSH security group, EC2 key pair, and stable public IPv4 address for each host. Port 22 is limited to the public IPv4 address observed when `provision.sh` runs. Public IPv4 addresses incur an AWS hourly charge in addition to instance and storage charges.

## Provision

Prerequisites are AWS CLI v2, Infisical CLI, `curl`, `ssh`, and `ssh-keygen`. Authenticate both CLIs first, then run:

```sh
cd tools/bench/aws-linux-ssh
AWS_PROFILE=boundaryml-dev ./provision.sh
```

The script guards against deploying to an unexpected account, defaults to account `147997132427` and region `us-east-1`, generates an Ed25519 key if needed, and stores it as `SAM_MICROGC_TEST_SSH_PRIVATE_KEY` and `SAM_MICROGC_TEST_SSH_PUBLIC_KEY` in Infisical project `bdd280e2-259c-4750-9b16-a8597a67214c`, environment `dev-humans`. It then deploys the stack and verifies SSH plus the reported machine architecture. Rerun it after your public IP changes to update the SSH allowlist without recreating the instances.

Set `SSH_CIDR` explicitly when automatic public-IP discovery is not suitable:

```sh
AWS_PROFILE=boundaryml-dev SSH_CIDR=203.0.113.10/32 ./provision.sh
```

## Connect

The wrapper retrieves the private key into a mode-0600 temporary file and removes it on exit:

```sh
./ssh.sh x64
./ssh.sh graviton
./ssh.sh x64 -- uname -a
./ssh.sh graviton -- uname -a
```

Inspect instance state and addresses with:

```sh
AWS_PROFILE=boundaryml-dev ./status.sh
```

## Destroy

```sh
AWS_PROFILE=boundaryml-dev ./destroy.sh
```

The destroy script deletes the CloudFormation stack and its AWS resources, but intentionally retains the Infisical secrets so a later deployment can reuse the same SSH identity.
