import React from 'react';
import NavbarItemOriginal from '@theme-original/NavbarItem';
import { NavbarAskButton } from '../../components/AskBaml';

interface NavbarItemProps {
  type?: string;
  [key: string]: unknown;
}

export default function NavbarItem(props: NavbarItemProps): React.ReactElement {
  // Handle custom navbar item types
  if (props.type === 'custom-askAiButton') {
    return <NavbarAskButton />;
  }

  return <NavbarItemOriginal {...props} />;
}
