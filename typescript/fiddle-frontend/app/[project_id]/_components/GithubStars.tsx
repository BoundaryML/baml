import { Separator } from '@baml/playground-common/components/ui/separator'
import { Star } from 'lucide-react'
import Image from 'next/image'
import Link from 'next/link'

async function getStars(repoOwner: string, repoName: string) {
  try {
    const response = await fetch(`https://api.github.com/repos/${repoOwner}/${repoName}`)
    const data = await response.json()
    return data.stargazers_count
  } catch (error) {
    console.error(error)
  }
}

export const GithubStars = () => {
  // const [stars, setStars] = useState<number>(170)
  // useEffect(() => {
  //   getStars('boundaryml', 'baml').then((stars) => setStars(stars))
  // }, [])

  return (
    <div>
      <Link
        className='flex flex-row items-center p-1 text-sm text-base font-light leading-6 group text-zinc-300 gap-x-2 hover:text-gray-100 '
        href='https://github.com/boundaryml/baml'
        target='_blank'
      >
        <Image
          src='/github-mark.svg'
          className='text-white fill-slate-400 hover:fill-slate-50'
          width={18}
          height={18}
          alt='Github'
        />
        <span className='hidden whitespace-nowrap 2xl:block'>Star us on Github</span>
        <Separator orientation='vertical' className=' border-zinc-100 w-[1px] h-[24px] bg-zinc-700' />
        <Star className=' group-hover:fill-slate-50' size={16} />
      </Link>
    </div>
  )
}
