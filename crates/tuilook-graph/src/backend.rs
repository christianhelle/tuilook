use std::collections::HashSet;

use serde::de::DeserializeOwned;
use tuilook_auth::TokenSource;
use tuilook_model::{BackendError, DeltaPage, Folder, FolderId, MailBackend};

use crate::{
    GraphClient, GraphError,
    dto::{self, Page},
};

const FOLDER_SELECT: &str =
    "$select=id,displayName,parentFolderId,childFolderCount,unreadItemCount,totalItemCount";

/// [`MailBackend`] over Microsoft Graph.
pub struct GraphBackend<T> {
    client: GraphClient<T>,
}

impl<T: TokenSource> GraphBackend<T> {
    pub fn new(client: GraphClient<T>) -> Self {
        Self { client }
    }

    /// Follows `@odata.nextLink` until the collection is exhausted.
    async fn get_all<R: DeserializeOwned>(&self, path: &str) -> Result<Vec<R>, GraphError> {
        let mut items = Vec::new();
        let mut next = Some(path.to_owned());
        while let Some(url) = next {
            let page: Page<R> = self.client.get(&url, &[]).await?;
            items.extend(page.value);
            next = page.next_link;
        }
        Ok(items)
    }
}

impl<T: TokenSource> MailBackend for GraphBackend<T> {
    async fn list_folders(&self) -> Result<Vec<Folder>, BackendError> {
        let mut graph_folders: Vec<dto::MailFolder> = self
            .get_all(&format!("/me/mailFolders?$top=100&{FOLDER_SELECT}"))
            .await?;
        // Breadth-first: append each folder's children as it is reached.
        let mut i = 0;
        while i < graph_folders.len() {
            if graph_folders[i].child_folder_count > 0 {
                let path = format!(
                    "/me/mailFolders/{}/childFolders?$top=100&{FOLDER_SELECT}",
                    graph_folders[i].id
                );
                let children = self.get_all(&path).await?;
                graph_folders.extend(children);
            }
            i += 1;
        }
        let ids: HashSet<_> = graph_folders.iter().map(|f| f.id.clone()).collect();
        Ok(graph_folders
            .into_iter()
            .map(|f| Folder {
                // Top-level folders hang off the hidden root; treat them as roots.
                parent_id: f.parent_folder_id.filter(|p| ids.contains(p)).map(FolderId),
                id: FolderId(f.id),
                name: f.display_name,
                well_known: None,
                unread: f.unread_item_count,
                total: f.total_item_count,
            })
            .collect())
    }

    async fn delta_sync(
        &self,
        _folder: &FolderId,
        _delta_link: Option<&str>,
    ) -> Result<DeltaPage, BackendError> {
        todo!()
    }
}

impl From<GraphError> for BackendError {
    fn from(e: GraphError) -> Self {
        BackendError(e.to_string())
    }
}
