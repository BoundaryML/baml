'use client';

import {
  CheckCircle,
  Clock,
  Cloud,
  Database,
  Rocket,
  Server,
  Shield,
} from 'lucide-react';
import { motion } from 'motion/react';
import { useEffect, useRef, useState } from 'react';
import { cn } from '@/lib/utils';

interface CloudProvider {
  id: string;
  name: string;
  icon: React.ReactNode;
  color: string;
  status: 'pending' | 'deploying' | 'success' | 'failed';
}

interface DeploymentStatusProps {
  className?: string;
  autoStart?: boolean;
  showProgress?: boolean;
}

export function DeploymentStatus({
  className,
  autoStart = true,
  showProgress = true,
}: DeploymentStatusProps) {
  const [currentProviderIndex, setCurrentProviderIndex] = useState(0);
  const [currentStepIndex, setCurrentStepIndex] = useState(0);
  const [isRunning, setIsRunning] = useState(false);
  const [overallProgress, setOverallProgress] = useState(0);
  const [hasStarted, setHasStarted] = useState(false);
  const [isComplete, setIsComplete] = useState(false);

  const animationStartedRef = useRef(false);
  const timeoutRef = useRef<NodeJS.Timeout | null>(null);
  const currentProviderRef = useRef(0);
  const currentStepRef = useRef(0);

  const cloudProviders: CloudProvider[] = [
    {
      color: 'from-orange-500 to-red-500',
      icon: <Server className="size-4" />,
      id: 'aws',
      name: 'AWS Lambda',
      status: 'pending',
    },
    {
      color: 'from-black to-gray-700',
      icon: <Rocket className="size-4" />,
      id: 'vercel',
      name: 'Vercel',
      status: 'pending',
    },
    {
      color: 'from-blue-500 to-blue-600',
      icon: <Cloud className="size-4" />,
      id: 'gcp',
      name: 'Google Cloud',
      status: 'pending',
    },
    {
      color: 'from-blue-600 to-blue-700',
      icon: <Database className="size-4" />,
      id: 'azure',
      name: 'Azure Functions',
      status: 'pending',
    },
    {
      color: 'from-purple-500 to-purple-600',
      icon: <Shield className="size-4" />,
      id: 'railway',
      name: 'Railway',
      status: 'pending',
    },
  ];

  const deploymentSteps = [
    {
      description: 'Generating platform-specific functions',
      duration: 1500,
      id: 'build',
      name: 'Building Native Code',
    },
    {
      description: 'Uploading to your chosen provider',
      duration: 2000,
      id: 'deploy',
      name: 'Deploying to Cloud',
    },
    {
      description: 'Testing endpoints and functions',
      duration: 1200,
      id: 'verify',
      name: 'Verifying Deployment',
    },
  ];

  // Clear any existing timeout
  const clearCurrentTimeout = () => {
    if (timeoutRef.current) {
      clearTimeout(timeoutRef.current);
      timeoutRef.current = null;
    }
  };

  // Calculate total duration for progress tracking
  const totalSteps = cloudProviders.length * deploymentSteps.length;

  useEffect(() => {
    if (!autoStart || animationStartedRef.current) return;

    const startDeployment = () => {
      if (animationStartedRef.current) return;
      animationStartedRef.current = true;

      setHasStarted(true);
      setIsRunning(true);
      setCurrentProviderIndex(0);
      setCurrentStepIndex(0);
      setOverallProgress(0);
      currentProviderRef.current = 0;
      currentStepRef.current = 0;

      const processStep = () => {
        const currentStep = deploymentSteps[currentStepRef.current];
        const currentProvider = cloudProviders[currentProviderRef.current];

        // Update state to reflect current progress
        setCurrentProviderIndex(currentProviderRef.current);
        setCurrentStepIndex(currentStepRef.current);

        // Update progress
        const completedSteps =
          currentProviderRef.current * deploymentSteps.length +
          currentStepRef.current;
        const progress = (completedSteps / totalSteps) * 100;
        setOverallProgress(progress);

        // Set timeout for current step
        timeoutRef.current = setTimeout(() => {
          // Check if we should move to next step or next provider
          if (currentStepRef.current < deploymentSteps.length - 1) {
            // Move to next step within same provider
            currentStepRef.current += 1;
            processStep();
          } else if (currentProviderRef.current < cloudProviders.length - 1) {
            // Move to next provider, reset step
            currentProviderRef.current += 1;
            currentStepRef.current = 0;
            processStep();
          } else {
            // All providers and steps completed
            setIsRunning(false);
            setIsComplete(true);
            setOverallProgress(100);
          }
        }, currentStep.duration);
      };

      // Start the deployment sequence
      processStep();
    };

    // Initial delay before starting
    timeoutRef.current = setTimeout(startDeployment, 800);

    return () => {
      clearCurrentTimeout();
    };
  }, [autoStart, cloudProviders, deploymentSteps, totalSteps]);

  const getProviderStatus = (
    providerIndex: number,
  ): CloudProvider['status'] => {
    if (!hasStarted) return 'pending';
    if (providerIndex < currentProviderIndex) return 'success';
    if (providerIndex === currentProviderIndex && isRunning) return 'deploying';
    return 'pending';
  };

  const getStatusIcon = (status: CloudProvider['status']) => {
    switch (status) {
      case 'success':
        return <CheckCircle className="size-4 text-green-500" />;
      case 'deploying':
        return (
          <motion.div
            animate={{ rotate: 360 }}
            transition={{
              duration: 1.5,
              ease: 'linear',
              repeat: Number.POSITIVE_INFINITY,
            }}
          >
            <Clock className="size-4 text-blue-500" />
          </motion.div>
        );
      case 'failed':
        return <CheckCircle className="size-4 text-red-500" />;
      default:
        return <div className="size-4 rounded-full border-2 border-gray-300" />;
    }
  };

  const getCurrentStep = () => {
    if (!isRunning) return null;
    return deploymentSteps[currentStepIndex];
  };

  const getCurrentProvider = () => {
    if (!isRunning) return null;
    return cloudProviders[currentProviderIndex];
  };

  return (
    <div className={cn('w-full max-w-2xl', className)}>
      {/* Overall Progress Bar */}
      {showProgress && (
        <motion.div
          animate={{ opacity: 1, y: 0 }}
          className="mb-6"
          initial={{ opacity: 0, y: -10 }}
          transition={{ delay: 0.3, duration: 0.5 }}
        >
          <div className="flex justify-between items-center mb-2">
            <span className="text-sm font-medium text-gray-700 dark:text-gray-300">
              Multi-Cloud Deployment
            </span>
            <span className="text-sm text-gray-500 dark:text-gray-400">
              {Math.round(overallProgress)}%
            </span>
          </div>
          <div className="w-full h-2 bg-gray-200 dark:bg-gray-700 rounded-full overflow-hidden">
            <motion.div
              animate={{ width: `${overallProgress}%` }}
              className="h-full bg-gradient-to-r from-blue-500 via-purple-500 to-green-500 rounded-full"
              initial={{ width: 0 }}
              transition={{ duration: 0.8, ease: 'easeOut' }}
            />
          </div>
        </motion.div>
      )}

      {/* Cloud Providers Grid */}
      <div className="grid grid-cols-2 md:grid-cols-3 gap-4 mb-6">
        {cloudProviders.map((provider, index) => {
          const status = getProviderStatus(index);
          const isActive = status === 'deploying';
          const isCompleted = status === 'success';

          return (
            <motion.div
              animate={{ opacity: 1, scale: 1 }}
              className={cn(
                'relative p-4 rounded-lg border transition-all duration-500',
                isActive &&
                  'bg-blue-50 border-blue-200 dark:bg-blue-950/20 dark:border-blue-800 shadow-lg scale-105',
                isCompleted &&
                  'bg-green-50 border-green-200 dark:bg-green-950/20 dark:border-green-800',
                !isActive &&
                  !isCompleted &&
                  'bg-gray-50 border-gray-200 dark:bg-gray-900 dark:border-gray-700',
              )}
              initial={{ opacity: 0, scale: 0.9 }}
              key={provider.id}
              transition={{
                delay: hasStarted ? index * 0.1 : 0.1 + index * 0.1,
                duration: 0.6,
                ease: 'easeOut',
              }}
              whileHover={{ scale: 1.02 }}
            >
              {/* Background gradient for active provider */}
              {isActive && (
                <motion.div
                  animate={{ opacity: [0.1, 0.2, 0.1] }}
                  className={cn(
                    'absolute inset-0 rounded-lg bg-gradient-to-br opacity-10',
                    provider.color,
                  )}
                  transition={{ duration: 2, repeat: Number.POSITIVE_INFINITY }}
                />
              )}

              <div className="relative z-10">
                <div className="flex items-center justify-between mb-3">
                  <div
                    className={cn(
                      'flex items-center justify-center w-8 h-8 rounded-full border transition-colors duration-500',
                      isActive &&
                        'bg-blue-100 dark:bg-blue-900/30 border-blue-300 dark:border-blue-700',
                      isCompleted &&
                        'bg-green-100 dark:bg-green-900/30 border-green-300 dark:border-green-700',
                      !isActive &&
                        !isCompleted &&
                        'bg-white dark:bg-gray-800 border-gray-300 dark:border-gray-600',
                    )}
                  >
                    {getStatusIcon(status)}
                  </div>
                  {isActive && (
                    <motion.div
                      animate={{ scale: [1, 1.3, 1] }}
                      className="w-2 h-2 bg-blue-500 rounded-full"
                      transition={{
                        duration: 1.5,
                        ease: 'easeInOut',
                        repeat: Number.POSITIVE_INFINITY,
                      }}
                    />
                  )}
                </div>

                <div className="flex items-center space-x-2 mb-2">
                  <div
                    className={cn(
                      'p-1 rounded',
                      isActive && 'bg-blue-100 dark:bg-blue-900/30',
                      isCompleted && 'bg-green-100 dark:bg-green-900/30',
                      !isActive &&
                        !isCompleted &&
                        'bg-gray-100 dark:bg-gray-800',
                    )}
                  >
                    {provider.icon}
                  </div>
                  <span
                    className={cn(
                      'text-sm font-medium transition-colors duration-300',
                      isActive && 'text-blue-700 dark:text-blue-300',
                      isCompleted && 'text-green-700 dark:text-green-300',
                      !isActive &&
                        !isCompleted &&
                        'text-gray-600 dark:text-gray-400',
                    )}
                  >
                    {provider.name}
                  </span>
                </div>

                {isActive && (
                  <motion.div
                    animate={{ width: '100%' }}
                    className="h-1 bg-blue-200 dark:bg-blue-800 rounded-full overflow-hidden"
                    initial={{ width: 0 }}
                    transition={{
                      delay: 0.2,
                      duration: 4.7, // Total duration for all steps
                      ease: 'easeInOut',
                    }}
                  >
                    <motion.div
                      animate={{ width: '100%' }}
                      className={cn(
                        'h-full rounded-full bg-gradient-to-r',
                        provider.color,
                      )}
                      initial={{ width: 0 }}
                      transition={{
                        delay: 0.2,
                        duration: 4.7,
                        ease: 'easeInOut',
                      }}
                    />
                  </motion.div>
                )}
              </div>
            </motion.div>
          );
        })}
      </div>

      {/* Current Step Info */}
      {isRunning && getCurrentProvider() && getCurrentStep() && (
        <motion.div
          animate={{ opacity: 1, y: 0 }}
          className="mb-6 p-4 bg-blue-50 dark:bg-blue-950/20 border border-blue-200 dark:border-blue-800 rounded-lg"
          initial={{ opacity: 0, y: 10 }}
          transition={{ delay: 0.5, duration: 0.5 }}
        >
          <div className="flex items-center space-x-3">
            <motion.div
              animate={{ scale: [1, 1.1, 1] }}
              transition={{ duration: 2, repeat: Number.POSITIVE_INFINITY }}
            >
              <Clock className="size-5 text-blue-500" />
            </motion.div>
            <div>
              <div className="text-sm font-medium text-blue-700 dark:text-blue-300">
                Deploying to {getCurrentProvider()?.name}
              </div>
              <div className="text-xs text-blue-600 dark:text-blue-400">
                {getCurrentStep()?.name} - {getCurrentStep()?.description}
              </div>
            </div>
          </div>
        </motion.div>
      )}

      {/* Success Message */}
      {isComplete && (
        <motion.div
          animate={{ opacity: 1, scale: 1, y: 0 }}
          className="p-6 bg-gradient-to-r from-green-50 to-emerald-50 dark:from-green-950/20 dark:to-emerald-950/20 border border-green-200 dark:border-green-800 rounded-lg text-center"
          initial={{ opacity: 0, scale: 0.95, y: 20 }}
          transition={{ delay: 0.8, duration: 0.6, ease: 'easeOut' }}
        >
          <motion.div
            animate={{ scale: [1, 1.1, 1] }}
            className="inline-block mb-3"
            transition={{ duration: 0.8, ease: 'easeInOut' }}
          >
            <CheckCircle className="size-8 text-green-500" />
          </motion.div>
          <h4 className="text-lg font-semibold text-green-700 dark:text-green-300 mb-2">
            Deployed Successfully!
          </h4>
          <p className="text-sm text-green-600 dark:text-green-400 mb-3">
            Your BAML project is now live across all cloud providers
          </p>
          <div className="flex flex-wrap justify-center gap-2">
            {cloudProviders.map((provider) => (
              <span
                className="px-2 py-1 text-xs bg-green-100 dark:bg-green-900/30 text-green-700 dark:text-green-300 rounded"
                key={provider.id}
              >
                {provider.name}
              </span>
            ))}
          </div>
        </motion.div>
      )}
    </div>
  );
}

// Compact version for smaller spaces
export function CompactDeploymentStatus({ className }: { className?: string }) {
  const [isDeployed, setIsDeployed] = useState(false);
  const timeoutRef = useRef<NodeJS.Timeout | null>(null);

  useEffect(() => {
    timeoutRef.current = setTimeout(() => setIsDeployed(true), 3000);
    return () => {
      if (timeoutRef.current) {
        clearTimeout(timeoutRef.current);
      }
    };
  }, []);

  return (
    <motion.div
      animate={{ opacity: 1, y: 0 }}
      className={cn(
        'flex items-center space-x-3 p-4 rounded-lg border bg-gradient-to-r from-gray-50 to-gray-100 dark:from-gray-900 dark:to-gray-800',
        className,
      )}
      initial={{ opacity: 0, y: 10 }}
      transition={{ delay: 0.2, duration: 0.5 }}
    >
      <motion.div
        animate={isDeployed ? { scale: [1, 1.1, 1] } : {}}
        transition={{ duration: 0.6, ease: 'easeInOut' }}
      >
        {isDeployed ? (
          <CheckCircle className="size-6 text-green-500" />
        ) : (
          <motion.div
            animate={{ rotate: 360 }}
            transition={{
              duration: 2,
              ease: 'linear',
              repeat: Number.POSITIVE_INFINITY,
            }}
          >
            <Clock className="size-6 text-blue-500" />
          </motion.div>
        )}
      </motion.div>

      <div>
        <div className="text-sm font-medium">
          {isDeployed ? 'Deployed Everywhere' : 'Deploying...'}
        </div>
        <div className="text-xs text-muted-foreground">
          {isDeployed
            ? 'Works on any cloud provider'
            : 'Building and deploying your AI'}
        </div>
      </div>
    </motion.div>
  );
}
