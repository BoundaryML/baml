/// <reference types="@docusaurus/module-type-aliases" />

declare module '@theme/Layout' {
  import type { ReactNode } from 'react';

  interface Props {
    children?: ReactNode;
    title?: string;
    description?: string;
  }

  export default function Layout(props: Props): JSX.Element;
}

declare module '@docusaurus/Link' {
  import type { ComponentProps } from 'react';

  interface Props extends ComponentProps<'a'> {
    to?: string;
  }

  export default function Link(props: Props): JSX.Element;
}

declare module '@docusaurus/useDocusaurusContext' {
  interface DocusaurusContext {
    siteConfig: {
      title: string;
      tagline: string;
      url: string;
      baseUrl: string;
      favicon?: string;
    };
  }

  export default function useDocusaurusContext(): DocusaurusContext;
}

declare module '*.module.css' {
  const classes: { [key: string]: string };
  export default classes;
}
