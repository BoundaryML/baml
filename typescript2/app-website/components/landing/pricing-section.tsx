'use client';

import { CheckIcon } from '@radix-ui/react-icons';
import { motion } from 'framer-motion';
import { Loader } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import { cn } from '@/lib/utils';

type Interval = 'month' | 'year';

export const toHumanPrice = (price: number, decimals = 2) => {
  return Number(price / 100).toFixed(decimals);
};
const demoPrices = [
  {
    description: 'Local .baml files for individual developers',
    features: [
      'First-class LLM functions',
      'Jinja-templated prompts',
      'Generated clients',
      'testset blocks',
    ],
    id: 'price_1',
    isMostPopular: false,
    monthlyPrice: 0,
    name: 'Open Source',
    yearlyPrice: 0,
  },
  {
    description: 'Shared typed prompt boundaries for teams',
    features: [
      'Structured-output parsing',
      'Team projects',
      'Typed catch patterns',
      'Generated Python and TypeScript clients',
      'Priority support',
    ],
    id: 'price_2',
    isMostPopular: true,
    monthlyPrice: 2000,
    name: 'Team',
    yearlyPrice: 20000,
  },
  {
    description: 'Structured-output AI boundaries for large organizations',
    features: [
      'Provider configuration review',
      '24/7 dedicated support',
      'Unlimited projects',
      'Schema-aware parser support',
      'Custom integrations',
      'Data security and compliance',
    ],
    id: 'price_5',
    isMostPopular: false,
    monthlyPrice: 5000,
    name: 'Enterprise',
    yearlyPrice: 50000,
  },
  {
    description: 'For teams pushing BAML in the wild',
    features: [
      'Friction notes and RFC loop',
      'White-glove support',
      'Unlimited projects',
      'Priority language feedback',
      'Custom integrations',
      'Highest data security and compliance',
    ],
    id: 'price_6',
    isMostPopular: false,
    monthlyPrice: 8000,
    name: 'Design Partner',
    yearlyPrice: 80000,
  },
];

export default function PricingSection() {
  const [interval, setInterval] = useState<Interval>('month');
  const [isLoading, setIsLoading] = useState(false);
  const [id, setId] = useState<string | null>(null);

  const onSubscribeClick = async (priceId: string) => {
    setIsLoading(true);
    setId(priceId);
    await new Promise((resolve) => setTimeout(resolve, 1000)); // Simulate a delay
    setIsLoading(false);
  };

  return (
    <section id="pricing">
      <div className="mx-auto flex max-w-screen-xl flex-col gap-8 px-4 py-14 md:px-8">
        <div className="mx-auto max-w-5xl text-center">
          <h4 className="text-xl font-bold tracking-tight text-black dark:text-white">
            Pricing
          </h4>

          <h2 className="text-5xl font-bold tracking-tight text-black dark:text-white sm:text-6xl">
            Pricing for typed LLM boundaries.
          </h2>

          <p className="mt-6 text-xl leading-8 text-black/80 dark:text-white">
            Start with first-class LLM functions locally. Upgrade when your team
            needs shared workflows around .baml files, generated clients, and
            structured-output parsing.
          </p>
        </div>

        <div className="flex w-full items-center justify-center space-x-2">
          <Switch
            id="interval"
            onCheckedChange={(checked) => {
              setInterval(checked ? 'year' : 'month');
            }}
          />
          <span>Annual</span>
          <span className="inline-block whitespace-nowrap rounded-full bg-black px-2.5 py-1 text-[11px] font-semibold uppercase leading-5 tracking-wide text-white dark:bg-white dark:text-black">
            2 MONTHS FREE ✨
          </span>
        </div>

        <div className="mx-auto grid w-full justify-center sm:grid-cols-2 lg:grid-cols-4 flex-col gap-4">
          {demoPrices.map((price, idx) => (
            <div
              className={cn(
                'relative flex max-w-[400px] flex-col gap-8 rounded-2xl border p-4 text-black dark:text-white overflow-hidden',
                {
                  'border-2 border-[var(--color-one)] dark:border-[var(--color-one)]':
                    price.isMostPopular,
                },
              )}
              key={price.id}
            >
              <div className="flex items-center">
                <div className="ml-4">
                  <h2 className="text-base font-semibold leading-7">
                    {price.name}
                  </h2>
                  <p className="h-12 text-sm leading-5 text-black/70 dark:text-white">
                    {price.description}
                  </p>
                </div>
              </div>

              <motion.div
                animate="animate"
                className="flex flex-row gap-1"
                initial="initial"
                key={`${price.id}-${interval}`}
                transition={{
                  delay: 0.1 + idx * 0.05,
                  duration: 0.4,
                  ease: [0.21, 0.47, 0.32, 0.98],
                }}
                variants={{
                  animate: {
                    opacity: 1,
                    y: 0,
                  },
                  initial: {
                    opacity: 0,
                    y: 12,
                  },
                }}
              >
                <span className="text-4xl font-bold text-black dark:text-white">
                  $
                  {interval === 'year'
                    ? toHumanPrice(price.yearlyPrice, 0)
                    : toHumanPrice(price.monthlyPrice, 0)}
                  <span className="text-xs"> / {interval}</span>
                </span>
              </motion.div>

              <Button
                className={cn(
                  'group relative w-full gap-2 overflow-hidden text-lg font-semibold tracking-tighter',
                  'transform-gpu ring-offset-current transition-all duration-300 ease-out hover:ring-2 hover:ring-primary hover:ring-offset-2',
                )}
                disabled={isLoading}
                onClick={() => void onSubscribeClick(price.id)}
              >
                <span className="absolute right-0 -mt-12 h-32 w-8 translate-x-12 rotate-12 transform-gpu bg-white opacity-10 transition-all duration-1000 ease-out group-hover:-translate-x-96 dark:bg-black" />
                {(!isLoading || (isLoading && id !== price.id)) && (
                  <p>Subscribe</p>
                )}

                {isLoading && id === price.id && <p>Subscribing</p>}
                {isLoading && id === price.id && (
                  <Loader className="mr-2 h-4 w-4 animate-spin" />
                )}
              </Button>

              <hr className="m-0 h-px w-full border-none bg-gradient-to-r from-neutral-200/0 via-neutral-500/30 to-neutral-200/0" />
              {price.features && price.features.length > 0 && (
                <ul className="flex flex-col gap-2 font-normal">
                  {price.features.map((feature: string) => (
                    <li
                      className="flex items-center gap-3 text-xs font-medium text-black dark:text-white"
                      key={feature}
                    >
                      <CheckIcon className="h-5 w-5 shrink-0 rounded-full bg-green-400 p-[2px] text-black dark:text-white" />
                      <span className="flex">{feature}</span>
                    </li>
                  ))}
                </ul>
              )}
            </div>
          ))}
        </div>
      </div>
    </section>
  );
}
