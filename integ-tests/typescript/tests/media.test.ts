import { b } from './test-setup'
import { Image, Audio } from '@boundaryml/baml'
import { image_b64, audio_b64 } from './base64_test_data'

describe('Media Tests', () => {
  it('should work with image from url', async () => {
    let res = await b.TestImageInput(
      Image.fromUrl('https://upload.wikimedia.org/wikipedia/en/4/4d/Shrek_%28character%29.png'),
    )
    expect(res.toLowerCase()).toMatch(/(green|yellow|ogre|shrek)/)
  })

  it('should work with image from base 64', async () => {
    let res = await b.TestImageInput(Image.fromBase64('image/png', image_b64))
    expect(res.toLowerCase()).toMatch(/(green|yellow|ogre|shrek)/)
  })

  it('should work with audio base 64', async () => {
    let res = await b.AudioInput(Audio.fromBase64('audio/mp3', audio_b64))
    expect(res.toLowerCase()).toContain('yes')
  })

  it('should work with audio from url', async () => {
    let res = await b.AudioInput(
      Audio.fromUrl('https://actions.google.com/sounds/v1/emergency/beeper_emergency_call.ogg'),
    )

    expect(res.toLowerCase()).toContain('no')
  })
})
