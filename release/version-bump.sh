#!/bin/sh

set -e


CURRENT_BRANCH=$(git branch --show-current)
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
  while true; do
    if [[ "${COMPONENT}" == "VSCode Extension" ]]; then
      echo "Enter the version type for ${COMPONENT} [none, patch, minor, major] (none):"
      VALID_COMPONENT_VERSIONS=("none" "patch" "minor" "major")
    else
      echo "Enter the version type for ${COMPONENT} [none, prerelease, pre, patch, minor, major] (none):"
      VALID_COMPONENT_VERSIONS=("none" "prerelease" "pre" "patch" "minor" "major")
    fi
    read VERSION_TYPE
    if [[ -z "${VERSION_TYPE}" ]]; then
      VERSION_TYPE="none"
    fi
    
    if [[ " ${VALID_COMPONENT_VERSIONS[@]} " =~ " ${VERSION_TYPE} " ]]; then
      if [[ "${COMPONENT}" == "CLI" ]]; then
        CLI=${VERSION_TYPE}
      elif [[ "${COMPONENT}" == "Python Client" ]]; then
        CLIENT_PYTHON=${VERSION_TYPE}
      else
        VSCODE_EXT=${VERSION_TYPE}
      fi
      break
    else
      echo "Invalid version type. Please enter a valid version type."
    fi
  done
done

if [ "$CLI" != "none" ] || [ "$CLIENT_PYTHON" != "none" ] || [ "$VSCODE_EXT" != "none" ]
then
  TIMESTAMP=$(date +%s%3N)
  git checkout -b ${USER}/bump-version/${TIMESTAMP}

  if [ "$CLI" != "none" ]
  then
    pushd engine
    VERSION=$(bumpversion --allow-dirty $CLI --list | grep new_version | cut -d '=' -f 2) || exit 1
    cargo build
    COMMIT_MSG="${COMMIT_MSG} [BUMP:cli:${VERSION}]"
    popd
  fi
  
  if [ "$CLIENT_PYTHON" != "none" ]
  then
    pushd clients/python
    VERSION=$(bumpversion --allow-dirty $CLIENT_PYTHON --list | grep new_version | cut -d '=' -f 2) || exit 1
    COMMIT_MSG="${COMMIT_MSG} [BUMP:py_client:${VERSION}]"
    popd
  fi
  
  if [ "$VSCODE_EXT" != "none" ]
  then
    pushd typescript
    VERSION=$(bumpversion --allow-dirty $VSCODE_EXT --list | grep new_version | cut -d '=' -f 2) || exit 1
    COMMIT_MSG="${COMMIT_MSG} [BUMP:vscode_ext:${VERSION}]"
    popd
  fi
  
  git commit -am "${COMMIT_MSG}"
  gh pr create --title "${COMMIT_MSG}" --body "Automated flow to bump version${COMMIT_MSG}"
  git checkout ${CURRENT_BRANCH}
  git branch -D ${USER}/bump-version/${TIMESTAMP}
fi
