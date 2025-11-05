declare module 'react-svg-pan-zoom' {
  import * as React from 'react';

  export const POSITION_NONE: 'none';
  export const TOOL_NONE: 'none';
  export const TOOL_AUTO: 'auto';

  export interface ReactSvgPanZoomInstance {
    pan(SVGDeltaX: number, SVGDeltaY: number): void;
    zoom(SVGPointX: number, SVGPointY: number, scaleFactor: number): void;
    fitSelection(selectionSVGPointX: number, selectionSVGPointY: number, selectionWidth: number, selectionHeight: number): void;
    fitToViewer(alignX?: string, alignY?: string): void;
    zoomOnViewerCenter(scaleFactor: number): void;
    setPointOnViewerCenter(SVGPointX: number, SVGPointY: number, zoomLevel: number): void;
    reset(): void;
    openMiniature(): void;
    closeMiniature(): void;
  }

  export interface UncontrolledReactSVGPanZoomProps {
    width: number;
    height: number;
    defaultValue?: object;
    defaultTool?: string;
    detectAutoPan?: boolean;
    toolbarProps?: {
      position?: string;
    };
    miniatureProps?: {
      position?: string;
    };
    background?: string;
    children: React.ReactElement;
    [key: string]: unknown;
  }

  export const UncontrolledReactSVGPanZoom: React.ForwardRefExoticComponent<
    UncontrolledReactSVGPanZoomProps & React.RefAttributes<ReactSvgPanZoomInstance>
  >;
}
