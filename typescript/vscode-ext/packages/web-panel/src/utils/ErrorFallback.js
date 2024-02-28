"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
var react_error_boundary_1 = require("react-error-boundary");
var ErrorFallback = function (_a) {
    var error = _a.error, resetErrorBoundary = _a.resetErrorBoundary;
    return (<div role="alert">
      <p>Something went wrong:</p>
      <pre>{error.message}</pre>
      <button onClick={resetErrorBoundary}>Try again</button>
    </div>);
};
var CustomErrorBoundary = function (_a) {
    var children = _a.children;
    return (<react_error_boundary_1.ErrorBoundary FallbackComponent={ErrorFallback} onReset={function () {
            // Reset the state of your app so the error doesn't happen again
        }}>
      {children}
    </react_error_boundary_1.ErrorBoundary>);
};
exports.default = CustomErrorBoundary;
