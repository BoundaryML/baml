export interface Author {
  name: string
  imageUrl?: string
  linkedin?: string
}

export interface OpenGraph {
  // Basic Metadata (Required)
  title?: string
  type?: string
  image?:
    | string
    | {
        url: string
        secure_url?: string
        type?: string
        width?: number
        height?: number
        alt?: string
      }
  url?: string

  // Optional Metadata
  audio?:
    | string
    | {
        url: string
        secure_url?: string
        type?: string
      }
  description?: string
  determiner?: 'a' | 'an' | 'the' | '' | 'auto'
  locale?: string
  locale_alternate?: string[]
  site_name?: string
  video?:
    | string
    | {
        url: string
        secure_url?: string
        type?: string
        width?: number
        height?: number
      }

  // Vertical: Article
  article_published_time?: string // datetime
  article_modified_time?: string // datetime
  article_expiration_time?: string // datetime
  article_author?: string | string[]
  article_section?: string
  article_tag?: string[]

  // Vertical: Profile
  profile_first_name?: string
  profile_last_name?: string
  profile_username?: string
  profile_gender?: string

  // Vertical: Video.Movie
  video_actor?: string[] // array of profile URLs
  video_actor_role?: string
  video_director?: string[] // array of profile URLs
  video_writer?: string[] // array of profile URLs
  video_duration?: number // integer >=1
  video_release_date?: string // datetime
  video_tag?: string[]

  // Vertical: Video.Episode
  video_series?: string // URL to video.tv_show
}

export interface Post {
  title: string
  description: string
  slug: string
  date: string
  tags: string[]
  body: string
  isPublished?: boolean
  lastModified?: number
  author?: Author
  og?: OpenGraph
}
