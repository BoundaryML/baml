"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
var react_1 = require("react");
var react_dom_1 = require("react-dom");
var App_1 = require("./App");
var ErrorFallback_1 = require("./utils/ErrorFallback");
try {
    react_dom_1.default.render(<react_1.default.StrictMode>
      <ErrorFallback_1.default>
        <App_1.default />
      </ErrorFallback_1.default>
    </react_1.default.StrictMode>, document.getElementById('root'));
}
catch (error) {
    console.error(error);
    console.error('REACT error:' + JSON.stringify(error, null, 2));
}
