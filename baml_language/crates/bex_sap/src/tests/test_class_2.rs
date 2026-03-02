use super::*;

// --- Task classes ---
// class ServerActionTask { type "server_action", name string, description string, signature string @alias("function_signature") }
// class PageTask { type "page", name string, description string, required_components string[], required_actions string[], route string }
// class ComponentTask { type "component", name string, description string, props string }

fn make_task_db() -> (
    TyResolved<'static, &'static str>,
    TyResolved<'static, &'static str>,
    TyResolved<'static, &'static str>,
    TypeRefDb<'static, &'static str>,
) {
    let server_action = class_ty(
        "ServerActionTask",
        vec![
            field("type", literal_string("server_action")),
            field("name", string_ty()),
            field("description", string_ty()),
            field_with_aliases("signature", string_ty(), vec!["function_signature"]),
        ],
    );
    let page = class_ty(
        "PageTask",
        vec![
            field("type", literal_string("page")),
            field("name", string_ty()),
            field("description", string_ty()),
            field("required_components", array_of(annotated(string_ty()))),
            field("required_actions", array_of(annotated(string_ty()))),
            field("route", string_ty()),
        ],
    );
    let component = class_ty(
        "ComponentTask",
        vec![
            field("type", literal_string("component")),
            field("name", string_ty()),
            field("description", string_ty()),
            field("props", string_ty()),
        ],
    );

    let mut db = TypeRefDb::new();
    assert!(
        db.try_add("ServerActionTask", server_action.clone())
            .is_ok()
    );
    assert!(db.try_add("PageTask", page.clone()).is_ok());
    assert!(db.try_add("ComponentTask", component.clone()).is_ok());

    (server_action, page, component, db)
}

#[test]
fn test_single_page_task() {
    let (_, page, _, db) = make_task_db();

    let raw = r#"
{
      type: page,
      name: HomePage,
      description: Landing page with post list,
      required_components: [PostCard, PostFilter],
      required_actions: [fetchPosts],
      route: /
}
  "#;
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(page.as_ref(), &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(page.as_ref(), &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!({
        "type": "page",
        "name": "HomePage",
        "description": "Landing page with post list",
        "required_components": ["PostCard", "PostFilter"],
        "required_actions": ["fetchPosts"],
        "route": "/"
    });
    assert_eq!(json_value, expected);
}

#[test]
fn test_class_2_single() {
    let (_, _, _, db) = make_task_db();
    let target_ty = array_of(annotated(union_of(vec![
        annotated(Ty::Unresolved("ServerActionTask")),
        annotated(Ty::Unresolved("PageTask")),
        annotated(Ty::Unresolved("ComponentTask")),
    ])));

    let raw = r#"[
    {
      type: server_action,
      name: fetchPosts,
      description: Fetch paginated blog posts with sorting and filtering,
      function_signature: async function fetchPosts(page: number, sort: string, filters: object): Promise<PostList>
    }
  ]"#;
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty.as_ref(), &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty.as_ref(), &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!([
        {
            "type": "server_action",
            "name": "fetchPosts",
            "description": "Fetch paginated blog posts with sorting and filtering",
            "signature": "async function fetchPosts(page: number, sort: string, filters: object): Promise<PostList>"
        }
    ]);
    assert_eq!(json_value, expected);
}

#[test]
fn test_class_2_two() {
    let (_, _, _, db) = make_task_db();
    let target_ty = array_of(annotated(union_of(vec![
        annotated(Ty::Unresolved("ServerActionTask")),
        annotated(Ty::Unresolved("PageTask")),
        annotated(Ty::Unresolved("ComponentTask")),
    ])));

    let raw = r#"[
    {
      type: server_action,
      name: fetchPosts,
      description: Fetch paginated blog posts with sorting and filtering,
      function_signature: async function fetchPosts(page: number, sort: string, filters: object): Promise<PostList>
    },
    {
      type: component,
      name: PostCard,
      description: Card component for displaying post preview on home page,
      props: {title: string, excerpt: string, author: Author, date: string, onClick: () => void}
    }
  ]"#;
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty.as_ref(), &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty.as_ref(), &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!([
        {
            "type": "server_action",
            "name": "fetchPosts",
            "description": "Fetch paginated blog posts with sorting and filtering",
            "signature": "async function fetchPosts(page: number, sort: string, filters: object): Promise<PostList>"
        },
        {
            "type": "component",
            "name": "PostCard",
            "description": "Card component for displaying post preview on home page",
            "props": "{title: string, excerpt: string, author: Author, date: string, onClick: () => void}"
        }
    ]);
    assert_eq!(json_value, expected);
}

#[test]
fn test_class_2_three() {
    let (_, _, _, db) = make_task_db();
    let target_ty = array_of(annotated(union_of(vec![
        annotated(Ty::Unresolved("ServerActionTask")),
        annotated(Ty::Unresolved("PageTask")),
        annotated(Ty::Unresolved("ComponentTask")),
    ])));

    let raw = r#"[
    {
      type: server_action,
      name: fetchPosts,
      description: Fetch paginated blog posts with sorting and filtering,
      function_signature: async function fetchPosts(page: number, sort: string, filters: object): Promise<PostList>
    },
    {
      type: component,
      name: PostCard,
      description: Card component for displaying post preview on home page,
      props: {title: string, excerpt: string, author: Author, date: string, onClick: () => void}
    },
    {
      type: page,
      name: HomePage,
      description: Landing page with post list,
      required_components: [PostCard, PostFilter],
      required_actions: [fetchPosts],
      route: /
    }
  ]"#;
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty.as_ref(), &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty.as_ref(), &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!([
        {
            "type": "server_action",
            "name": "fetchPosts",
            "description": "Fetch paginated blog posts with sorting and filtering",
            "signature": "async function fetchPosts(page: number, sort: string, filters: object): Promise<PostList>"
        },
        {
            "type": "component",
            "name": "PostCard",
            "description": "Card component for displaying post preview on home page",
            "props": "{title: string, excerpt: string, author: Author, date: string, onClick: () => void}"
        },
        {
            "type": "page",
            "name": "HomePage",
            "description": "Landing page with post list",
            "required_components": ["PostCard", "PostFilter"],
            "required_actions": ["fetchPosts"],
            "route": "/"
        }
    ]);
    assert_eq!(json_value, expected);
}

#[test]
fn test_class_2_four() {
    let (_, _, _, db) = make_task_db();
    let target_ty = array_of(annotated(union_of(vec![
        annotated(Ty::Unresolved("ServerActionTask")),
        annotated(Ty::Unresolved("PageTask")),
        annotated(Ty::Unresolved("ComponentTask")),
    ])));

    let raw = r#"[
    {
      type: server_action,
      name: fetchPosts,
      description: Fetch paginated blog posts with sorting and filtering,
      function_signature: async function fetchPosts(page: number, sort: string, filters: object): Promise<PostList>
    },
    {
      type: component,
      name: PostCard,
      description: Card component for displaying post preview on home page,
      props: {title: string, excerpt: string, author: Author, date: string, onClick: () => void}
    },
    {
      type: page,
      name: HomePage,
      description: Landing page with post list,
      required_components: [PostCard, PostFilter],
      required_actions: [fetchPosts],
      route: /
    },
    {
      type: server_action,
      name: fetchPostById,
      description: Fetch single post with full content and metadata,
      function_signature: async function fetchPostById(id: string): Promise<Post>
    }
  ]"#;
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty.as_ref(), &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty.as_ref(), &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!([
        {
            "type": "server_action",
            "name": "fetchPosts",
            "description": "Fetch paginated blog posts with sorting and filtering",
            "signature": "async function fetchPosts(page: number, sort: string, filters: object): Promise<PostList>"
        },
        {
            "type": "component",
            "name": "PostCard",
            "description": "Card component for displaying post preview on home page",
            "props": "{title: string, excerpt: string, author: Author, date: string, onClick: () => void}"
        },
        {
            "type": "page",
            "name": "HomePage",
            "description": "Landing page with post list",
            "required_components": ["PostCard", "PostFilter"],
            "required_actions": ["fetchPosts"],
            "route": "/"
        },
        {
            "type": "server_action",
            "name": "fetchPostById",
            "description": "Fetch single post with full content and metadata",
            "signature": "async function fetchPostById(id: string): Promise<Post>"
        }
    ]);
    assert_eq!(json_value, expected);
}

#[test]
fn test_full() {
    let (_, _, _, db) = make_task_db();
    let target_ty = array_of(annotated(union_of(vec![
        annotated(Ty::Unresolved("ServerActionTask")),
        annotated(Ty::Unresolved("PageTask")),
        annotated(Ty::Unresolved("ComponentTask")),
    ])));

    let raw = r###"
Let me break this down page by page:

Page 1: Home (/)
---
Features:
- List of blog post previews with title, excerpt, author
- Sort posts by date/popularity
- Filter by category/tag
Data:
- List of posts with preview data
- Sort/filter state
Actions:
- Fetch posts with sorting/filtering
- Navigate to individual posts
---

Page 2: Post Detail (/post/[id])
---
Features:
- Full post content display
- Author information section
- Comments section with add/reply
Data:
- Complete post data
- Author details
- Comments thread
Actions:
- Fetch post by ID
- Fetch comments
- Add/delete comments
---

Page 3: New Post (/new)
---
Features:
- Rich text editor interface
- Image upload capability
- Live preview
Data:
- Post draft data
- Uploaded images
Actions:
- Create new post
- Upload images
- Save draft
---

Page 4: Profile (/profile)
---
Features:
- User profile information
- Profile picture
- User's posts list
- Edit capabilities
Data:
- User profile data
- User's posts
Actions:
- Fetch user data
- Update profile
- Upload profile picture
---

[
  {
    type: server_action,
    name: fetchPosts,
    description: Fetch paginated blog posts with sorting and filtering,
    function_signature: async function fetchPosts(page: number, sort: string, filters: object): Promise<PostList>
  },
  {
    type: server_action,
    name: fetchPostById,
    description: Fetch single post with full content and metadata,
    function_signature: async function fetchPostById(id: string): Promise<Post>
  },
  {
    type: server_action,
    name: createPost,
    description: Create new blog post with content and images,
    function_signature: async function createPost(title: string, content: string, images: File[]): Promise<Post>
  },
  {
    type: server_action,
    name: fetchComments,
    description: Fetch comments for a post,
    function_signature: async function fetchComments(postId: string): Promise<Comment[]>
  },
  {
    type: server_action,
    name: addComment,
    description: Add new comment to a post,
    function_signature: async function addComment(postId: string, content: string): Promise<Comment>
  },
  {
    type: server_action,
    name: fetchUserProfile,
    description: Fetch user profile data and posts,
    function_signature: async function fetchUserProfile(userId: string): Promise<UserProfile>
  },
  {
    type: server_action,
    name: updateProfile,
    description: Update user profile information,
    function_signature: async function updateProfile(userId: string, data: ProfileData): Promise<UserProfile>
  },
  {
    type: component,
    name: PostCard,
    description: Card component for displaying post preview on home page,
    props: "{title: string, excerpt: string, author: Author, date: string, onClick: () => void}"
  },
   {
    type: component,
    name: PostFilter,
    description: Filter and sort controls for post list,
    props: "{onFilterChange: (filters: object) => void, onSortChange: (sort: string) => void}"
  },
   {
    type: component,
    name: RichTextEditor,
    description: Rich text editor component for creating posts,
    props: "{value: string, onChange: (content: string) => void, onImageUpload: (file: File) => Promise<string>}"
  },
  {
    type: component,
    name: CommentSection,
    description: Comments display and input component,
    props: {comments: Comment[], postId: string, onAddComment: (content: string) => void}
  },
  {
    type: component,
    name: ProfileHeader,
    description: User profile information display component,
    props: "{user: UserProfile, isEditable: boolean, onEdit: () => void}"
  },
  {
    type: page,
    name: HomePage,
    description: Landing page with post list,
    required_components: [PostCard, PostFilter],
    required_actions: [fetchPosts],
    route: /
  },
  {
    type: page,
    name: PostDetailPage,
    description: Full post display page,
    required_components: [CommentSection],
    required_actions: [fetchPostById, fetchComments, addComment],
    route: /post/[id]
  },
  {
    type: page,
    name: NewPostPage,
    description: Create new post page,
    required_components: [RichTextEditor],
    required_actions: [createPost],
    route: /new
  },
  {
    type: page,
    name: ProfilePage,
    description: User profile page,
    required_components: [ProfileHeader, PostCard],
    required_actions: [fetchUserProfile, updateProfile],
    route: /profile
  }
]
  "###;
    let parsed = crate::jsonish::parse(raw, Default::default(), true).unwrap();
    let ctx = crate::deserializer::coercer::ParsingContext::new(target_ty.as_ref(), &db);
    let annots = TypeAnnotations::default();
    let target = TyWithMeta::new(target_ty.as_ref(), &annots);
    let result = TyResolvedRef::coerce(&ctx, target, &parsed);
    assert!(result.is_ok());
    let value = result.unwrap().unwrap();
    let json_value = serde_json::to_value(&value).unwrap();
    let expected = serde_json::json!([
        {
            "type": "server_action",
            "name": "fetchPosts",
            "description": "Fetch paginated blog posts with sorting and filtering",
            "signature": "async function fetchPosts(page: number, sort: string, filters: object): Promise<PostList>"
        },
        {
            "type": "server_action",
            "name": "fetchPostById",
            "description": "Fetch single post with full content and metadata",
            "signature": "async function fetchPostById(id: string): Promise<Post>"
        },
        {
            "type": "server_action",
            "name": "createPost",
            "description": "Create new blog post with content and images",
            "signature": "async function createPost(title: string, content: string, images: File[]): Promise<Post>"
        },
        {
            "type": "server_action",
            "name": "fetchComments",
            "description": "Fetch comments for a post",
            "signature": "async function fetchComments(postId: string): Promise<Comment[]>"
        },
        {
            "type": "server_action",
            "name": "addComment",
            "description": "Add new comment to a post",
            "signature": "async function addComment(postId: string, content: string): Promise<Comment>"
        },
        {
            "type": "server_action",
            "name": "fetchUserProfile",
            "description": "Fetch user profile data and posts",
            "signature": "async function fetchUserProfile(userId: string): Promise<UserProfile>"
        },
        {
            "type": "server_action",
            "name": "updateProfile",
            "description": "Update user profile information",
            "signature": "async function updateProfile(userId: string, data: ProfileData): Promise<UserProfile>"
        },
        {
            "type": "component",
            "name": "PostCard",
            "description": "Card component for displaying post preview on home page",
            "props": "{title: string, excerpt: string, author: Author, date: string, onClick: () => void}"
        },
        {
            "type": "component",
            "name": "PostFilter",
            "description": "Filter and sort controls for post list",
            "props": "{onFilterChange: (filters: object) => void, onSortChange: (sort: string) => void}"
        },
        {
            "type": "component",
            "name": "RichTextEditor",
            "description": "Rich text editor component for creating posts",
            "props": "{value: string, onChange: (content: string) => void, onImageUpload: (file: File) => Promise<string>}"
        },
        {
            "type": "component",
            "name": "CommentSection",
            "description": "Comments display and input component",
            "props": "{comments: Comment[], postId: string, onAddComment: (content: string) => void}"
        },
        {
            "type": "component",
            "name": "ProfileHeader",
            "description": "User profile information display component",
            "props": "{user: UserProfile, isEditable: boolean, onEdit: () => void}"
        },
        {
            "type": "page",
            "name": "HomePage",
            "description": "Landing page with post list",
            "required_components": ["PostCard", "PostFilter"],
            "required_actions": ["fetchPosts"],
            "route": "/"
        },
        {
            "type": "page",
            "name": "PostDetailPage",
            "description": "Full post display page",
            "required_components": ["CommentSection"],
            "required_actions": ["fetchPostById", "fetchComments", "addComment"],
            "route": "/post/[id]"
        },
        {
            "type": "page",
            "name": "NewPostPage",
            "description": "Create new post page",
            "required_components": ["RichTextEditor"],
            "required_actions": ["createPost"],
            "route": "/new"
        },
        {
            "type": "page",
            "name": "ProfilePage",
            "description": "User profile page",
            "required_components": ["ProfileHeader", "PostCard"],
            "required_actions": ["fetchUserProfile", "updateProfile"],
            "route": "/profile"
        }
    ]);
    assert_eq!(json_value, expected);
}

// Skipped tests:
// - test_streaming_not_null_list (uses streaming behavior annotations)
// - test_streaming_not_null_list_partial (uses streaming behavior annotations)
// - test_streaming_not_null_list_partial_2 (uses streaming behavior annotations)
// - test_streaming_not_null_list_partial_3_0 (uses streaming behavior annotations)
// - test_streaming_not_null_list_partial_3_1 (uses streaming behavior annotations)
