'use client';

import {
  Carousel,
  CarouselContent,
  CarouselItem,
  CarouselNext,
  CarouselPrevious,
} from '@/components/ui/carousel';
import { cn } from '@/lib/utils';
import Image from 'next/image';
import * as React from 'react';

interface SpotlightUser {
  name: string;
  company: string;
  role: string;
  companyDescription: string;
  bamlTestimonial: string;
  imageUrl?: string;
  companyUrl?: string;
  logoUrl?: string;
  logoWidth?: number;
  logoHeight?: number;
}

interface DevSpotlightProps {
  users: SpotlightUser[];
}

const UserCard = ({ user }: { user: SpotlightUser }) => (
  <div className="rounded-lg border border-gray-100 bg-white p-6 shadow-md hover:shadow-lg transition-shadow duration-300 relative h-full">
    <div className="absolute inset-0 bg-gradient-to-br from-blue-50/30 to-transparent pointer-events-none" />
    <div className="relative">
      <div className="mb-4 flex items-start gap-4">
        {user.imageUrl && (
          <div className="relative w-14 h-14 md:w-16 md:h-16 rounded-full overflow-hidden flex-shrink-0 flex items-center justify-center bg-white">
            <Image
              src={user.imageUrl}
              alt={`${user.name}'s profile picture`}
              width={64}
              height={64}
              className="rounded-full object-cover"
            />
          </div>
        )}
        <div className="flex-1 min-w-0 self-center">
          <h3 className="text-base md:text-lg font-semibold text-gray-800 truncate leading-tight m-0">
            {user.name}
          </h3>
          <div className="text-xs md:text-sm text-gray-600 flex flex-wrap items-center gap-2 mt-0.5">
            <span className="font-medium truncate">{user.role}</span>
            <span className="text-gray-400 flex-shrink-0">•</span>
            {user.companyUrl ? (
              <a
                href={user.companyUrl}
                target="_blank"
                rel="noopener noreferrer"
                className="text-blue-600 hover:text-blue-800 truncate transition-colors underline"
              >
                {user.company}
              </a>
            ) : (
              <span className="text-blue-600 truncate">{user.company}</span>
            )}
          </div>
        </div>
      </div>

      <div className="space-y-4 md:space-y-6">
        <div>
          <h4 className="mb-2 text-sm md:text-base font-medium text-gray-800">
            What does your company do?
          </h4>
          <p className="text-sm text-gray-700 leading-relaxed text-muted-foreground text-pretty">
            {user.companyDescription}
          </p>
        </div>

        <div>
          <h4 className="mb-2 text-sm md:text-base font-medium text-gray-800">
            How has BAML helped you?
          </h4>
          <p className="text-sm text-gray-700 leading-relaxed text-muted-foreground text-pretty">
            {user.bamlTestimonial}
          </p>
        </div>

        {user.logoUrl && (
          <div className="flex justify-center items-center pt-2">
            <Image
              src={user.logoUrl}
              alt={`${user.company} logo`}
              width={user.logoWidth || 180}
              height={user.logoHeight || 60}
              className="object-contain max-h-[60px]"
            />
          </div>
        )}
      </div>
    </div>
  </div>
);

export function DevSpotlight({ users = [] }: DevSpotlightProps) {
  const [currentSlide, setCurrentSlide] = React.useState(0);

  return (
    <div className="-mx-4 md:-mx-8 not-prose">
      <div className="mb-8 rounded-lg border border-gray-100 bg-purple-50 px-4 md:px-8 pb-8 shadow-lg">
        <h2 className="mt-6 mb-2 md:mb-3 text-2xl md:text-3xl font-extrabold text-transparent bg-clip-text bg-gradient-to-r from-purple-900 via-purple-700 to-purple-500 text-center font-montserrat tracking-tight">
          Developer Spotlights
        </h2>
        <p className="text-sm md:text-base text-center text-gray-600 mb-6 md:mb-8 italic">
          Launch Week isn&apos;t just about us - it&apos;s also about the
          incredible engineering work of amazing companies building with BAML!
        </p>

        {/* Desktop Grid */}
        <div className="hidden md:grid md:grid-cols-2 gap-6">
          {users.map((user, index) => (
            <UserCard key={index} user={user} />
          ))}
        </div>

        {/* Mobile Carousel */}
        <div className="md:hidden w-full">
          <Carousel
            className="w-full"
            opts={{
              align: 'start',
              loop: false,
            }}
            setApi={(api) => {
              api?.on('select', () => {
                setCurrentSlide(api.selectedScrollSnap());
              });
            }}
          >
            <CarouselContent className="-mx-0 flex">
              {users.map((user, index) => (
                <CarouselItem key={index} className="pl-0 basis-full">
                  <div className="px-0">
                    <UserCard user={user} />
                  </div>
                </CarouselItem>
              ))}
            </CarouselContent>
            <CarouselPrevious className="-left-6" />
            <CarouselNext className="-right-2" />
          </Carousel>

          {/* Pagination Dots */}
          <div className="flex justify-center gap-2 mt-4">
            {users.map((_, index) => (
              <div
                key={index}
                className={cn(
                  'w-2 h-2 rounded-full transition-colors',
                  currentSlide === index ? 'bg-blue-500' : 'bg-gray-300',
                )}
              />
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
