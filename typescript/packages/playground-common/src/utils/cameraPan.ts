/**
 * Camera Pan Utilities
 *
 * Implements video-game-style camera tracking where the camera only moves
 * when the target gets close to the viewport edges (deadzone system).
 */

import type { Node, ReactFlowInstance } from '@xyflow/react';

/**
 * Configuration for camera deadzone
 * Values are percentages of viewport dimensions (0-1)
 */
interface DeadzoneConfig {
  /** Left edge threshold (default 0.25 = 25% from left) */
  left: number;
  /** Right edge threshold (default 0.75 = 75% from left) */
  right: number;
  /** Top edge threshold (default 0.25 = 25% from top) */
  top: number;
  /** Bottom edge threshold (default 0.75 = 75% from top) */
  bottom: number;
}

const DEFAULT_DEADZONE: DeadzoneConfig = {
  left: 0.15,   // 15% from left
  right: 0.85,  // 85% from left (15% from right)
  top: 0.15,    // 15% from top
  bottom: 0.85, // 85% from top (15% from bottom)
};

/**
 * Pan camera to keep node in view, only if it's outside the deadzone
 *
 * @param node - The target node to track
 * @param flowInstance - React Flow instance
 * @param config - Deadzone configuration (optional)
 * @param duration - Pan animation duration in ms (default 500)
 */
export function panToNodeIfNeeded(
  node: Node,
  flowInstance: ReactFlowInstance,
  config: DeadzoneConfig = DEFAULT_DEADZONE,
  duration = 250
): void {
  // Get viewport dimensions
  const viewport = flowInstance.getViewport();

  // Get viewport dimensions from the DOM
  const viewportElement = document.querySelector('.react-flow__viewport')?.parentElement;
  if (!viewportElement) {
    console.warn('📹 Cannot find viewport element');
    return;
  }

  const viewportWidth = viewportElement.clientWidth;
  const viewportHeight = viewportElement.clientHeight;

  // Calculate deadzone bounds in screen coordinates
  const deadzoneLeft = viewportWidth * config.left;
  const deadzoneRight = viewportWidth * config.right;
  const deadzoneTop = viewportHeight * config.top;
  const deadzoneBottom = viewportHeight * config.bottom;

  // Get node dimensions
  const nodeWidth = node.measured?.width ?? node.width ?? 100;
  const nodeHeight = node.measured?.height ?? node.height ?? 50;

  // Calculate node center in viewport coordinates
  const nodeCenterX = (node.position.x + nodeWidth / 2) * viewport.zoom + viewport.x;
  const nodeCenterY = (node.position.y + nodeHeight / 2) * viewport.zoom + viewport.y;

  // Check if node is outside deadzone
  let panX = 0;
  let panY = 0;

  // Check horizontal bounds
  const isLeftOfDeadzone = nodeCenterX < deadzoneLeft;
  const isRightOfDeadzone = nodeCenterX > deadzoneRight;
  const isAboveDeadzone = nodeCenterY < deadzoneTop;
  const isBelowDeadzone = nodeCenterY > deadzoneBottom;


  if (isLeftOfDeadzone) {
    // Node is too far left, pan camera right to bring it into view
    panX = deadzoneLeft - nodeCenterX;
  } else if (isRightOfDeadzone) {
    // Node is too far right, pan camera left to bring it into view
    panX = deadzoneRight - nodeCenterX;
  }

  if (isAboveDeadzone) {
    // Node is too far up, pan camera down to bring it into view
    panY = deadzoneTop - nodeCenterY;
  } else if (isBelowDeadzone) {
    // Node is too far down, pan camera up to bring it into view
    panY = deadzoneBottom - nodeCenterY;
  }

  // Only pan if node is outside deadzone
  if (panX !== 0 || panY !== 0) {


    flowInstance.setViewport(
      {
        x: viewport.x + panX,
        y: viewport.y + panY,
        zoom: viewport.zoom, // Keep zoom unchanged
      },
      { duration }
    );
  } else {
    console.log('📹 Node already in deadzone, no panning needed:', node.id);
  }
}

/**
 * Force pan camera to center on a node (no deadzone check)
 * Use this for initial navigation or explicit "go to" actions
 */
export function panToNodeCenter(
  node: Node,
  flowInstance: ReactFlowInstance,
  duration = 500
): void {
  // Get viewport dimensions from the DOM
  const viewportElement = document.querySelector('.react-flow__viewport')?.parentElement;
  if (!viewportElement) {
    console.warn('📹 Cannot find viewport element');
    return;
  }

  const viewport = flowInstance.getViewport();
  const viewportWidth = viewportElement.clientWidth;
  const viewportHeight = viewportElement.clientHeight;

  // Calculate node center
  const nodeWidth = node.measured?.width ?? node.width ?? 100;
  const nodeHeight = node.measured?.height ?? node.height ?? 50;
  const nodeCenterX = node.position.x + nodeWidth / 2;
  const nodeCenterY = node.position.y + nodeHeight / 2;

  // Calculate viewport center
  const viewportCenterX = viewportWidth / 2;
  const viewportCenterY = viewportHeight / 2;

  // Calculate required pan to center node
  const panX = viewportCenterX - nodeCenterX * viewport.zoom;
  const panY = viewportCenterY - nodeCenterY * viewport.zoom;


  flowInstance.setViewport(
    {
      x: panX,
      y: panY,
      zoom: viewport.zoom, // Keep zoom unchanged
    },
    { duration }
  );
}
