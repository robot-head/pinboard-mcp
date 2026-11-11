use lettre::{
    AsyncSmtpTransport, AsyncTransport, Message, Tokio1Executor,
    transport::smtp::authentication::Credentials,
};
use std::sync::Arc;

#[derive(Clone, Default)]
pub struct Mailer {
    transport: Option<Arc<AsyncSmtpTransport<Tokio1Executor>>>,
    from: String,
}
impl Mailer {
    pub fn from_env() -> anyhow::Result<Self> {
        let Ok(host) = std::env::var("PINBOARD_SMTP_HOST") else {
            return Ok(Self::default());
        };
        let mode = std::env::var("PINBOARD_SMTP_TLS").unwrap_or_else(|_| "starttls".into());
        let port = std::env::var("PINBOARD_SMTP_PORT")
            .ok()
            .map(|v| v.parse::<u16>())
            .transpose()?
            .unwrap_or(if mode == "tls" { 465 } else { 587 });
        let from = std::env::var("PINBOARD_SMTP_FROM")
            .map_err(|_| anyhow::anyhow!("PINBOARD_SMTP_FROM is required with SMTP"))?;
        let _: lettre::message::Mailbox = from.parse()?;
        let builder = match mode.as_str() {
            "tls" => AsyncSmtpTransport::<Tokio1Executor>::relay(&host)?,
            "starttls" => AsyncSmtpTransport::<Tokio1Executor>::starttls_relay(&host)?,
            "none" => {
                anyhow::ensure!(
                    matches!(host.as_str(), "127.0.0.1" | "localhost" | "::1"),
                    "Unencrypted SMTP is allowed only on loopback for tests and local development"
                );
                AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&host)
            }
            _ => anyhow::bail!("PINBOARD_SMTP_TLS must be starttls, tls, or none"),
        };
        let mut builder = builder
            .port(port)
            .timeout(Some(std::time::Duration::from_secs(15)));
        if let Ok(username) = std::env::var("PINBOARD_SMTP_USERNAME") {
            let password = std::env::var("PINBOARD_SMTP_PASSWORD").map_err(|_| {
                anyhow::anyhow!("PINBOARD_SMTP_PASSWORD is required with an SMTP username")
            })?;
            builder = builder.credentials(Credentials::new(username, password));
        }
        Ok(Self {
            transport: Some(Arc::new(builder.build())),
            from,
        })
    }
    pub fn available(&self) -> bool {
        self.transport.is_some()
    }
    pub async fn invitation(
        &self,
        to: &str,
        title: &str,
        inviter: &str,
        link: &str,
    ) -> anyhow::Result<()> {
        let transport = self
            .transport
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Configure SMTP to send email invitations"))?;
        let message=Message::builder().from(self.from.parse()?).to(to.parse()?).subject(format!("Invitation to a Pinboard canvas: {}",title.chars().take(120).collect::<String>())).body(format!("{inviter} invited you to collaborate on “{title}”.\n\nOpen this link and sign in with the invited email address:\n{link}\n\nThis invitation expires in 7 days.\n"))?;
        transport.send(message).await?;
        Ok(())
    }
}
