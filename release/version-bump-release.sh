#!/bin/sh

set -e


CURRENT_BRANCH=$(git branch --show-current)
if [[ "${CURRENT_BRANCH}" != "canary" ]]; then
  echo "You are not on the 'canary' branch. Please switch to the 'canary' branch before proceeding."
  exit 1
fi
git fetch origin > /dev/null 2>&1
if [[ $(git diff --stat) != '' ]]; then
  echo "Your repo is dirty. Please commit your changes before bumping the version."
  exit 1
fi
if [[ $(git diff --stat origin/${CURRENT_BRANCH}) != '' ]]; then
  echo "Your branch is not in sync with origin. Please sync before bumping the version."
  exit 1
fi

for COMPONENT in "CLI" "Python Client" "VSCode Extension"; do
    if [[ "${COMPONENT}" == "VSCode Extension" ]]; then
        echo "Release ${COMPONENT}? [Y/N]:"
        read PATCH_VERSION
        if [[ "${PATCH_VERSION}" =~ ^[Yy]$ ]]; then
            VSCODE_EXT="minor"
        else
            VSCODE_EXT="none"
        fi
    else
        echo "Release ${COMPONENT}? [Y/N]:"
        read PRERELEASE_VERSION
        if [[ "${PRERELEASE_VERSION}" =~ ^[Yy]$ ]]; then
            if [[ "${COMPONENT}" == "CLI" ]]; then
                CLI="minor"
            elif [[ "${COMPONENT}" == "Python Client" ]]; then
                CLIENT_PYTHON="minor"
            fi
        else
            if [[ "${COMPONENT}" == "CLI" ]]; then
                CLI="none"
            elif [[ "${COMPONENT}" == "Python Client" ]]; then
                CLIENT_PYTHON="none"
            fi
        fi
    fi
done

if [ "$CLI" != "none" ] || [ "$CLIENT_PYTHON" != "none" ] || [ "$VSCODE_EXT" != "none" ]
then
  TIMESTAMP=$(date +%s%3N)
  git checkout -b ${USER}/release-version/${TIMESTAMP}

  COMMIT_MSG="Release"
  if [ "$CLI" != "none" ]
  then
    pushd engine
    VERSION=$(bumpversion --allow-dirty pre --list | grep new_version | cut -d '=' -f 2) || exit 1
    cargo build
    COMMIT_MSG="${COMMIT_MSG} [BUMP:cli:${VERSION}]"
    popd
  fi
  
  if [ "$CLIENT_PYTHON" != "none" ]
  then
    pushd clients/python
    VERSION=$(bumpversion --allow-dirty pre --list | grep new_version | cut -d '=' -f 2) || exit 1
    COMMIT_MSG="${COMMIT_MSG} [BUMP:py_client:${VERSION}]"
    popd
  fi
  
  if [ "$VSCODE_EXT" != "none" ]
  then
    pushd typescript
    VERSION=$(bumpversion --allow-dirty minor --list | grep new_version | cut -d '=' -f 2) || exit 1
    COMMIT_MSG="${COMMIT_MSG} [BUMP:vscode_ext:${VERSION}]"
    popd
  fi
  
  git commit -am "${COMMIT_MSG}"

  COMMIT_MSG=""
  # Now bump up to the minor version
  if [ "$CLI" != "none" ]
  then
    pushd engine
    VERSION=$(bumpversion --allow-dirty minor --list | grep new_version | cut -d '=' -f 2) || exit 1
    cargo build
    COMMIT_MSG="${COMMIT_MSG} [BUMP:cli:${VERSION}]"
    popd
  fi
  
  if [ "$CLIENT_PYTHON" != "none" ]
  then
    pushd clients/python
    VERSION=$(bumpversion --allow-dirty minor --list | grep new_version | cut -d '=' -f 2) || exit 1
    COMMIT_MSG="${COMMIT_MSG} [BUMP:py_client:${VERSION}]"
    popd
  fi
  
  if [ "$VSCODE_EXT" != "none" ]
  then
    pushd typescript
    VERSION=$(bumpversion --allow-dirty minor --list | grep new_version | cut -d '=' -f 2) || exit 1
    COMMIT_MSG="${COMMIT_MSG} [BUMP:vscode_ext:${VERSION}]"
    popd
  fi
  git commit -am "${COMMIT_MSG}"
  
  gh pr create --title "${COMMIT_MSG}" --body "Automated flow to bump version${COMMIT_MSG}"
  git checkout ${CURRENT_BRANCH}
  git branch -D ${USER}/release-version/${TIMESTAMP}
fi
