/** @type {import('tailwindcss').Config} */

export default {
  content: ['./index.html', './src/**/*.{js,jsx,ts,tsx}'],
  theme: {
    colors: {
      contrastBorder: 'var(--vscode-contrastBorder)',
    },
  },

  plugins: [
    require('@githubocto/tailwind-vscode'),
    // function ({ addUtilities, theme, e }) {
    //   const colors = theme('colors')
    //   let newUtilities = {}
    //   Object.keys(colors).forEach((key) => {
    //     const value = colors[key]
    //     const name = `.border-${key}`
    //     newUtilities[name] = { borderColor: value }
    //   })
    //   console.log(newUtilities)
    //   addUtilities(newUtilities)
    // },
  ],
}
