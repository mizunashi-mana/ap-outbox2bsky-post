use activitist::{json::JsonSerde, model as ap_model};
use async_trait::async_trait;
use std::error::Error;

#[async_trait]
pub trait LinkResolver {
    async fn resolve_json<T: JsonSerde>(&self, url: &str) -> Result<T, Box<dyn Error>>;
}

pub struct Outbox(ap_model::Object);

impl Outbox {
    pub fn from_json_bytes(bytes: &[u8]) -> Result<Self, Box<dyn Error>> {
        let object = ap_model::Object::from_json_bytes(bytes)?;
        Ok(Self(object))
    }

    pub async fn activity_items<Resolver: LinkResolver>(
        &self,
        resolver: &Resolver,
        mut max_items: usize,
    ) -> Result<Vec<Activity>, Box<dyn Error>> {
        let mut items = Vec::new();

        if self.0.collection_items.total_items.is_some_and(|x| x == 0) {
            return Ok(items);
        }

        if max_items == 0 {
            return Ok(items);
        }

        for item in &self.0.collection_items.items {
            match item {
                ap_model::ObjectOrLink::Object(object) => items.push(Activity(object.clone())),
                ap_model::ObjectOrLink::Link(link) => {
                    let object = resolver.resolve_json(&link.href).await?;
                    items.push(Activity(object));
                }
            }
            max_items -= 1;

            if max_items == 0 {
                return Ok(items);
            }
        }

        for item in &self.0.ordered_collection_items.ordered_items {
            match item {
                ap_model::ObjectOrLink::Object(object) => items.push(Activity(object.clone())),
                ap_model::ObjectOrLink::Link(link) => {
                    let object = resolver.resolve_json(&link.href).await?;
                    items.push(Activity(object));
                }
            }
            max_items -= 1;

            if max_items == 0 {
                return Ok(items);
            }
        }

        if !items.is_empty() {
            return Ok(items);
        }

        match &self.0.collection_items.first {
            None => Ok(items),
            Some(first) => {
                let items = Vec::new();
                Self::fetch_activities(*first.clone(), items, resolver, max_items).await
            }
        }
    }

    async fn fetch_activities<Resolver: LinkResolver>(
        mut next_ref: ap_model::ObjectOrLink,
        mut items: Vec<Activity>,
        resolver: &Resolver,
        mut max_items: usize,
    ) -> Result<Vec<Activity>, Box<dyn Error>> {
        loop {
            let next_page = match &next_ref {
                ap_model::ObjectOrLink::Object(object) => object.clone(),
                ap_model::ObjectOrLink::Link(link) => resolver.resolve_json(&link.href).await?,
            };

            if max_items == 0 {
                return Ok(items);
            }

            for item in &next_page.collection_items.items {
                match item {
                    ap_model::ObjectOrLink::Object(object) => items.push(Activity(object.clone())),
                    ap_model::ObjectOrLink::Link(link) => {
                        let object = resolver.resolve_json(&link.href).await?;
                        items.push(Activity(object));
                    }
                }
                max_items -= 1;

                if max_items == 0 {
                    return Ok(items);
                }
            }

            for item in &next_page.ordered_collection_items.ordered_items {
                match item {
                    ap_model::ObjectOrLink::Object(object) => items.push(Activity(object.clone())),
                    ap_model::ObjectOrLink::Link(link) => {
                        let object = resolver.resolve_json(&link.href).await?;
                        items.push(Activity(object));
                    }
                }
                max_items -= 1;

                if max_items == 0 {
                    return Ok(items);
                }
            }

            if {
                next_page.collection_items.items.is_empty()
                    && next_page.ordered_collection_items.ordered_items.is_empty()
            } {
                return Ok(items);
            }

            match next_page.collection_page_items.next {
                None => return Ok(items),
                Some(next) => next_ref = *next,
            }
        }
    }
}

#[derive(Debug)]
pub struct Activity(ap_model::Object);

impl Activity {
    pub fn is_create(&self) -> bool {
        for typ in &self.0.typ {
            match typ.as_str() {
                "Create" => return true,
                "Announce" => {
                    // do nothing
                }
                _ => {
                    eprintln!("Not supported type: {typ}")
                }
            }
        }

        false
    }

    pub async fn item<Resolver: LinkResolver>(
        &self,
        resolver: &Resolver,
    ) -> Result<Option<Item>, Box<dyn Error>> {
        match self.0.activity_items.object.first() {
            None => Ok(None),
            Some(ap_model::ObjectOrLink::Object(object)) => Ok(Some(Item(object.clone()))),
            Some(ap_model::ObjectOrLink::Link(link)) => {
                let object = resolver.resolve_json(&link.href).await?;
                Ok(Some(Item(object)))
            }
        }
    }
}

#[derive(Debug)]
pub struct Item(ap_model::Object);

impl Item {
    pub fn id(&self) -> Option<&str> {
        match &self.0.id {
            Some(x) => Some(x),
            None => None,
        }
    }

    pub fn url(&self) -> Option<&str> {
        match &self.0.object_items.url {
            Some(x) => Some(&x.href),
            None => self.id(),
        }
    }

    pub fn content(&self) -> Option<&str> {
        self.0.object_items.content.first().map(|x| x.as_str())
    }

    pub fn is_reply(&self) -> bool {
        !self.0.object_items.in_reply_to.is_empty()
    }

    pub fn is_sensitive(&self) -> bool {
        self.0.activity_streams_ext_items.sensitive.unwrap_or(false)
    }

    pub async fn attachments<Resolver: LinkResolver>(
        &self,
        resolver: &Resolver,
    ) -> Result<Vec<Attachment>, Box<dyn Error>> {
        let mut result = Vec::with_capacity(self.0.object_items.attachment.len());
        for attachment in &self.0.object_items.attachment {
            match attachment {
                ap_model::ObjectOrLink::Object(object) => result.push(Attachment(object.clone())),
                ap_model::ObjectOrLink::Link(link) => {
                    let object = resolver.resolve_json(&link.href).await?;
                    result.push(Attachment(object));
                }
            }
        }
        Ok(result)
    }
}

pub struct Attachment(ap_model::Object);

impl Attachment {
    pub fn is_media(&self) -> bool {
        for media_type in &self.0.object_items.media_type {
            match media_type.as_str() {
                "image/png" | "image/jpeg" => return true,
                _ => {
                    eprintln!("Not supported media type: {media_type}")
                }
            }
        }

        false
    }

    pub fn url(&self) -> Option<&str> {
        match &self.0.object_items.url {
            Some(x) => Some(&x.href),
            None => None,
        }
    }
}
