import type { Graph } from './types';

export const roadmapData: Graph = {
  nodes: [
    { id: 'start', label: 'Pick a Programming Language', kind: 'item' },
    { id: 'os', label: 'Operating System', kind: 'item' },

    { id: 'linux', label: 'Linux', kind: 'group' },
    { id: 'linux1', label: 'Ubuntu / Debian', kind: 'item', parent: 'linux' },
    { id: 'linux2', label: 'Fedora / RHEL', kind: 'item', parent: 'linux' },

    { id: 'os2', label: 'Windows', kind: 'item' },
    { id: 'tk', label: 'Terminal Knowledge', kind: 'item' },

    { id: 'scripting', label: 'Scripting', kind: 'group' },
    {
      id: 'scripting1',
      label: 'Bash / Shell',
      kind: 'item',
      parent: 'scripting',
    },
    {
      id: 'scripting2',
      label: 'PowerShell',
      kind: 'item',
      parent: 'scripting',
    },

    { id: 'editors', label: 'Editors', kind: 'group' },
    {
      id: 'editors1',
      label: 'Vim / Nano / Emacs',
      kind: 'item',
      parent: 'editors',
    },

    { id: 'tktools', label: 'TKTools', kind: 'group' },
    {
      id: 'tk1',
      label: 'Network Basic Knowledge',
      kind: 'item',
      parent: 'tktools',
    },
    { id: 'tk2', label: 'Networking Tools', kind: 'item', parent: 'tktools' },
    { id: 'tk3', label: 'Text Manipulation', kind: 'item', parent: 'tktools' },
    { id: 'tk4', label: 'Debugging Tools', kind: 'item', parent: 'tktools' },

    { id: 'git', label: 'Version Control System', kind: 'item' },
    { id: 'git1', label: 'Git', kind: 'item' },
    { id: 'gitlab', label: 'GitLab', kind: 'item' },

    { id: 'containers', label: 'Containers', kind: 'item' },
    { id: 'containers1', label: 'Podman', kind: 'item' },
    { id: 'containers2', label: 'Docker', kind: 'item' },

    { id: 'deploy', label: 'What is and how to setup ... ?', kind: 'item' },
  ],
  edges: [
    { id: 'e_start_os', from: 'start', to: 'os', style: 'solid' },

    { id: 'e_os_linux', from: 'os', to: 'linux', style: 'dashed' },
    { id: 'e_os_os2', from: 'os', to: 'os2', style: 'dashed' },

    { id: 'e_os_tk', from: 'os', to: 'tk', style: 'solid' },
    { id: 'e_tk_scripting', from: 'tk', to: 'scripting', style: 'dashed' },
    { id: 'e_tk_editors', from: 'tk', to: 'editors', style: 'dashed' },
    { id: 'e_tk_tktools', from: 'tk', to: 'tktools', style: 'dashed' },

    { id: 'e_tk_git', from: 'tk', to: 'git', style: 'solid' },
    { id: 'e_git_git1', from: 'git', to: 'git1', style: 'dashed' },
    { id: 'e_git1_gitlab', from: 'git1', to: 'gitlab', style: 'dashed' },

    { id: 'e_git_containers', from: 'git', to: 'containers', style: 'solid' },
    {
      id: 'e_cont_podman',
      from: 'containers',
      to: 'containers1',
      style: 'dashed',
    },
    {
      id: 'e_cont_docker',
      from: 'containers',
      to: 'containers2',
      style: 'dashed',
    },
    { id: 'e_cont_deploy', from: 'containers', to: 'deploy', style: 'solid' },
  ],
};
