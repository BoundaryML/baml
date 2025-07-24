import Image from 'next/image';
import Link from 'next/link';

async function getStars(repoOwner: string, repoName: string) {
  try {
    const response = await fetch(
      `https://api.github.com/repos/${repoOwner}/${repoName}`,
    );
    const data = await response.json();
    return data.stargazers_count;
  } catch (error) {
    console.error(error);
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
        className="flex flex-row gap-x-2 items-center p-1 text-sm font-light leading-6 group hover:text-foreground/80"
        href="https://github.com/boundaryml/baml"
        target="_blank"
      >
        <Image
          src="/github-mark.svg"
          className="opacity-60 invert hover:opacity-100 dark:invert-0 dark:fill-white"
          width={18}
          height={18}
          alt="Github"
        />
        {/* <span className="hidden whitespace-nowrap text-primary-foreground/50 2xl:block">Star us on Github</span>
        <Separator orientation="vertical" className=" w-[1px] h-[24px] hidden" />
        <Star className="hidden" size={16} /> */}
      </Link>
    </div>
  );
};
