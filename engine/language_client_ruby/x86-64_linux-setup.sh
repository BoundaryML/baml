#!/bin/bash

set -euxo pipefail

# from https://serverfault.com/questions/1161816/mirrorlist-centos-org-no-longer-resolve
sudo sed -i 's/mirror.centos.org/vault.centos.org/g' /etc/yum.repos.d/*.repo
sudo sed -i 's/^#.*baseurl=http/baseurl=http/g'      /etc/yum.repos.d/*.repo
sudo sed -i 's/^mirrorlist=http/#mirrorlist=http/g'  /etc/yum.repos.d/*.repo

ls /etc/yum.repos.d/

 # We need this to build engine/, since it's needed for OpenSSL
 sudo yum install -y perl-IPC-Cmd