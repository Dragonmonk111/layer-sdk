# Deployment

Some tips on how to deploy this to a remote server, not the developer machine.

For now, we will still use docker compose to run it, so we just need to show how to
set everything up there.

## Prepare the server

Get an Ubuntu server, e.g. from Hetzner. This document assumes Ubuntu 22.04.
Give some space for compilation, so we suggest the following size: `4 vCPU, 8 GB RAM, 160 GB SSD` (Hetzner CPX31 for less than $20/month).

Everything below assumes an ssh shell on the server.

### Install docker

Follow instructions from https://docs.docker.com/engine/install/ubuntu/

Add Docker's official GPG key:

```bash
sudo apt-get update
sudo apt-get install -y ca-certificates curl gnupg
sudo install -m 0755 -d /etc/apt/keyrings
curl -fsSL https://download.docker.com/linux/ubuntu/gpg | sudo gpg --dearmor -o /etc/apt/keyrings/docker.gpg
sudo chmod a+r /etc/apt/keyrings/docker.gpg

# Add the repository to Apt sources:
echo \
  "deb [arch="$(dpkg --print-architecture)" signed-by=/etc/apt/keyrings/docker.gpg] https://download.docker.com/linux/ubuntu \
  "$(. /etc/os-release && echo "$VERSION_CODENAME")" stable" | \
  sudo tee /etc/apt/sources.list.d/docker.list > /dev/null
sudo apt-get update
```

Install Docker Engine:

```
sudo apt-get install -y docker-ce docker-ce-cli containerd.io docker-buildx-plugin docker-compose-plugin

sudo docker run hello-world
```

## Install the code

**Create an ssh key on the server:**

```bash
ssh-keygen
cat .ssh/id_rsa.pub
```

**Give access to the github repo to this ssh key:**

Go to https://github.com/Lay3rLabs/layer-sdk/settings/keys

"Add Deploy Key" with read-only access

**Clone code and build it:**

```bash
cd
git clone git@github.com:Lay3rLabs/layer-sdk.git
cd layer-sdk

./scripts/build_docker.sh
docker images

./scripts/reset_volumes.sh
docker volume ls
```

## Run the server

Open a screen and run the following inside it:

```
docker compose up
```

Then detach from the screen `(Ctrl+A D)`. Now, with a normal shell, you can check the logs:

```bash
docker compose logs cometbft
docker compose logs jaeger
```

View jaeger via web interface http://65.21.105.220:8080

Check RPC `curl http://65.21.105.220:26657/status | jq .`

### As a service

TODO

### nginx proxy

TODO: also with certbot for ssl certificates


## Test local client against it

```
cd integration
npm ci
# change hostName variable in testutils.spec.ts to the IP of the server
npm run test
```

Look at jaeger client to check the traces (for example `execute_tx`)
