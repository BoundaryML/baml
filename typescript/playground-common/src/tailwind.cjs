const path = require('path')

module.exports = {
  // `content` is replaced instead of extended, so this line has to be added in
  // the `content` of each app' tailwind.config.js
  content: [path.join(path.dirname(require.resolve('@baml/playground-common')), '**/*.{js,ts,jsx,tsx}')],
  theme: {
    extend: {
      
      opacity: {
        '15': '0.15',
        '35': '0.35',
        '65': '0.65',
       }

    },
  },
}
