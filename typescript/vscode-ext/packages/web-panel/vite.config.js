"use strict";
Object.defineProperty(exports, "__esModule", { value: true });
var vite_1 = require("vite");
var plugin_react_1 = require("@vitejs/plugin-react");
var path_1 = require("path");
var isWatchMode = process.argv.includes('--watch');
console.log('isWatchMode', isWatchMode);
// https://vitejs.dev/config/
exports.default = (0, vite_1.defineConfig)({
    plugins: (0, plugin_react_1.default)(),
    resolve: {
        alias: {
            '@': path_1.default.resolve(__dirname, './src'),
        },
    },
    mode: isWatchMode ? 'development' : 'production',
    build: {
        minify: isWatchMode ? false : true,
        outDir: 'dist',
        sourcemap: isWatchMode ? 'inline' : undefined,
        rollupOptions: {
            // external: ['allotment/dist/index.css'],
            output: {
                entryFileNames: "assets/[name].js",
                chunkFileNames: "assets/[name].js",
                assetFileNames: "assets/[name].[ext]",
            },
        },
    },
});
