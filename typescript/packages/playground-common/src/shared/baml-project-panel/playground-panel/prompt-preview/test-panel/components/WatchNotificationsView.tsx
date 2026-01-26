import React from 'react'
import { Badge } from '@baml/ui/badge'
import { Collapsible, CollapsibleContent, CollapsibleTrigger } from '@baml/ui/collapsible'
import { ChevronDown, Eye, Layers, Zap, Variable } from 'lucide-react'
import type { WatchNotification } from '../types'
import { getNotificationLabel, getNotificationType, getNotificationLogFilterKey } from '../utils/notifications'

interface WatchNotificationsViewProps {
  notifications?: WatchNotification[]
}

export function WatchNotificationsView({ notifications }: WatchNotificationsViewProps) {
  const [isOpen, setIsOpen] = React.useState(false)

  if (!notifications || notifications.length === 0) {
    return null
  }

  return (
    <Collapsible open={isOpen} onOpenChange={setIsOpen}>
      <CollapsibleTrigger className="flex items-center gap-2 text-sm font-medium py-2 w-full hover:bg-muted/50 rounded px-2">
        <ChevronDown className={`h-4 w-4 transition-transform ${isOpen ? 'rotate-180' : ''}`} />
        <Eye className="h-4 w-4" />
        <span>Watch Notifications</span>
        <Badge variant="secondary" className="ml-auto">
          {notifications.length}
        </Badge>
      </CollapsibleTrigger>
      <CollapsibleContent className="px-2 pb-2">
        <div className="space-y-1 mt-2">
          {/* Display all notifications in chronological order */}
          {notifications.map((notification, index) => (
            <NotificationItem key={index} notification={notification} index={index} />
          ))}
        </div>
      </CollapsibleContent>
    </Collapsible>
  )
}

function NotificationItem({ notification, index }: { notification: WatchNotification; index: number }) {
  const [isExpanded, setIsExpanded] = React.useState(false)
  const label = getNotificationLabel(notification)
  const type = getNotificationType(notification)

  // Choose icon and color based on type
  const getTypeConfig = () => {
    switch (type) {
      case 'variable':
        return {
          icon: Variable,
          variant: 'outline' as const,
          color: 'text-blue-600 dark:text-blue-400'
        }
      case 'block':
        return {
          icon: Layers,
          variant: 'secondary' as const,
          color: 'text-purple-600 dark:text-purple-400'
        }
      case 'stream':
        return {
          icon: Zap,
          variant: 'default' as const,
          color: 'text-yellow-600 dark:text-yellow-400'
        }
    }
  }

  const { icon: Icon, variant, color } = getTypeConfig()
  const blockKey = getNotificationLogFilterKey(notification)

  return (
    <div className="border rounded p-2 text-xs bg-muted/30 hover:bg-muted/50 transition-colors">
      <div
        className="flex items-center justify-between cursor-pointer"
        onClick={() => setIsExpanded(!isExpanded)}
      >
        <div className="flex items-center gap-2">
          {/* Event number for chronological reference */}
          <span className="text-muted-foreground font-mono">#{index + 1}</span>

          {/* Type badge with icon */}
          <Badge variant={variant} className="text-xs py-0 h-5 gap-1">
            <Icon className="h-3 w-3" />
            {type}
          </Badge>

          {/* Notification label */}
          <span className="font-medium">{label}</span>
        </div>

        {/* Channel name if present */}
        {notification.channelName && (
          <span className="text-muted-foreground text-xs">ch: {notification.channelName}</span>
        )}
      </div>

      {/* Expandable value section */}
      {isExpanded && (
        <div className="mt-2 pt-2 border-t">
          <div className="flex items-start gap-2">
            <span className="text-muted-foreground text-xs">Value:</span>
            <pre className="text-xs text-muted-foreground overflow-x-auto flex-1">
             {(() => {
                if (notification.value === undefined) return ''
                try {
                  // Try to parse as JSON and format it nicely
                  const parsed = JSON.parse(notification.value)
                  return JSON.stringify(parsed, null, 2)
                } catch {
                  // If not valid JSON, return as is
                  return notification.value
                }
              })()}
            </pre>
          </div>
          {blockKey && (
            <div className="mt-1">
              <span className="text-muted-foreground text-xs">Block: {blockKey}</span>
            </div>
          )}
        </div>
      )}
    </div>
  )
}
