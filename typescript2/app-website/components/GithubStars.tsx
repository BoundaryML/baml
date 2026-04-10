/* eslint-disable @typescript-eslint/no-unsafe-assignment */
/* eslint-disable @typescript-eslint/no-unsafe-member-access */
/* eslint-disable @typescript-eslint/no-floating-promises */
'use client';
import { Star } from 'lucide-react';
import Link from 'next/link';
import { useEffect, useState } from 'react';
import { FaGithub } from 'react-icons/fa';

async function getStars(repoOwner: string, repoName: string) {
  try {
    const response = await fetch(
      `https://api.github.com/repos/${repoOwner}/${repoName}`,
    );
    const data = await response.json();
    return data.stargazers_count as number;
  } catch (error) {
    console.error(error);
  }
  return 4999;
}

export const GithubStars = () => {
  const [stars, setStars] = useState<number | undefined>(undefined);
  const [hovering, setHovering] = useState<boolean>(false);

  useEffect(() => {
    getStars('boundaryml', 'baml').then((stars) => setStars(stars ?? 2107));
  }, []);

  const displayStars = stars ?? 2107;

  return (
    <Link
      className="group flex flex-row items-center justify-center gap-x-2 rounded-full border-[1px] border-[#30363d] bg-[#161b22] px-1.5 py-1 text-sm font-light leading-6 text-white transition duration-200 ease-in-out hover:scale-[1.05] hover:bg-zinc-900"
      href="https://github.com/boundaryml/baml"
      onMouseEnter={() => setHovering(true)}
      onMouseLeave={() => setHovering(false)}
      target="_blank"
    >
      <FaGithub className="text-white" size={18} />

      <span className="hidden sm:inline whitespace-nowrap">Star on GitHub</span>
      <div className="flex flex-row items-center justify-center gap-x-1">
        <Star className="mb-0.5" fill={'#ffffff'} size={16} />
        <span className="ml-0">
          {(hovering ? displayStars + 1 : displayStars).toLocaleString()}
        </span>
      </div>
    </Link>
  );
};
