const path = require('path');

module.exports = {
  entry: './src/index.tsx',
  output: {
    filename: 'custom.js',
    path: path.resolve(__dirname, '../../../fern'),
    library: 'FernChatbot',
    libraryTarget: 'umd',
    globalObject: 'this',
  },
  resolve: {
    extensions: ['.tsx', '.ts', '.js'],
  },
  module: {
    rules: [
      {
        test: /\.tsx?$/,
        use: 'ts-loader',
        exclude: /node_modules/,
      },
      {
        test: /\.svg$/,
        type: 'asset/inline',
      },
      {
        test: /\.css$/i,
        use: ['style-loader', 'css-loader'],
      },
    ],
  },
  externals: {
    // Don't bundle React if it's already available globally
    // react: 'React',
    // 'react-dom': 'ReactDOM'
  },
  optimization: {
    minimize: true,
  },
};
