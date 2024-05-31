import { Card, CardTitle } from '@/components/ui/card'
//import { useSelections } from '@baml/playground-common'
import { useAtom } from 'jotai'
import Link from 'next/link'
import { useParams } from 'next/navigation'
import Joyride, { type Placement, TooltipProps } from 'react-joyride'
import { productTourDoneAtom, productTourTestDoneAtom } from '../_atoms/atoms'

export const InitialTour = () => {
  const steps = [
    {
      target: '.tour-editor',
      content: (
        <>
          Welcome! PromptFiddle is a playground to share and test prompt templates.{' '}
          <span className='font-semibold'>Prompts here are modeled like functions</span>
        </>
      ),
      disableBeacon: true,
      placement: 'auto' as Placement,
    },
    // {
    //   // ..that can convert these definitions into actual Python or TS functions
    //   target: '.tour-editor',
    //   content: 'LLM functions are written using BAML, a superset of the Jinja language.',
    //   placement: 'right' as Placement,
    // },
    {
      target: '.tour-prompt-preview',
      content: (
        <div>
          <p>See a realtime preview of the exact prompt, even if you add loops, ifs, or change models</p>
          <br />
          <p className='font-semibold'> No guessing what the prompt is!</p>
        </div>
      ),
      placement: 'left' as Placement,
    },
    {
      target: '.tour-test-panel',
      content: (
        <>
          Click <span className='font-semibold'>'Run all'</span> to test this LLM function! Close this dialog by
          clicking next.
        </>
      ),
      placement: 'left-start' as Placement,
    },

    // {
    //   target: '.tour-templates',
    //   content: 'Check out other templates to learn different prompting strategies',
    // },
  ]
  const [productTourDone, setProductTourDone] = useAtom(productTourDoneAtom)
  if (productTourDone) {
    return null
  }

  return (
    <div className=''>
      <Joyride
        steps={steps}
        continuous={true}
        disableOverlayClose={true}
        spotlightClicks={true}
        showProgress={true}
        hideCloseButton={true}
        disableCloseOnEsc={true}
        showSkipButton={false}
        callback={(data) => {
          if (data.status === 'finished') {
            setProductTourDone(true)
          }
        }}
        styles={{
          options: {
            overlayColor: 'rgba(0, 0, 0, 0.7)',
          },
        }}
      />
    </div>
  )
}

export const PostTestRunTour = () => {
  //const { test_results, test_result_url, test_result_exit_status } = useSelections()
  const { project_id } = useParams()

  const steps = [
    {
      target: '.tour-test-result-panel',
      content: (
        <div>
          These are the test results! BAML calls the LLM and parses the output into your function output type.
          <br />
          <br />
          The JSON view is the <span className='font-semibold'>parsed output</span>.
          <br />
          <br />
          Click on <span className='font-semibold'>"Show raw output"</span> to see the string that the LLM returned.
        </div>
      ),
      disableBeacon: true,
      placement: 'left' as Placement,
    },

    // {
    //   target: '.tour-templates',
    //   content: 'Check out other templates to learn different prompting strategies',
    // },
  ]

  const [productTourTestDone, setProductTourTestDone] = useAtom(productTourTestDoneAtom)
  const [productTourDone] = useAtom(productTourDoneAtom)

  if (project_id === 'extract-resume' || project_id === undefined) {
    steps.push({
      // ..that can convert these definitions into actual Python or TS functions
      target: '.tour-file-view',
      content: (
        <div>
          Check out the main.py or main.ts file to see how the LLM function is called in Python or TypeScript. For more
          info, see the{' '}
          <Link className='text-blue-600' href='https://docs.boundaryml.com'>
            docs
          </Link>{' '}
          , or reach out on Discord.
          <br />
          <br />
          Happy prompting!
        </div>
      ),
      disableBeacon: false,
      placement: 'right' as Placement,
    })
  }

  if (!productTourDone) {
    return null
  }

  if (/*test_result_exit_status !== 'COMPLETED' || */ productTourTestDone) {
    return null
  }

  return (
    <div className=''>
      <Joyride
        steps={steps}
        continuous={true}
        disableOverlayClose={true}
        spotlightClicks={true}
        showProgress={true}
        hideCloseButton={true}
        disableCloseOnEsc={true}
        showSkipButton={false}
        callback={(data) => {
          if (data.status === 'finished') {
            setProductTourTestDone(true)
          }
        }}
        styles={{
          options: {
            overlayColor: 'rgba(0, 0, 0, 0.7)',
          },
        }}
      />
    </div>
  )
}
