use std::str::FromStr;

use builder_macro::Builder;
use tonic::transport::Endpoint;

#[derive(Builder)]
struct TransportBuilder<T = BoxedTransport> {
    pub transport: T,
}

struct BoxedTransport;

impl BoxedTransport {}

impl<T> TransportBuilderBuilder<T> {
    fn url(mut self, url: &str) -> anyhow::Result<TransportBuilderBuilder<Endpoint>> {
        let channel = Endpoint::from_str(url)?;
        //.tls_config(tls_config)?
        //.connect_lazy();
        Ok(self.with_transport(channel))
    }

    fn with_transport<T1>(mut self, transport: T1) -> TransportBuilderBuilder<T1> {
        TransportBuilderBuilder { transport }
    }
}

#[test]
fn basics() {
    // Can we just hoold it
    let mut builder = TransportBuilder::builder();

    // Can we change it
    builder = builder.url("hellow.orld").unwrap();

    let result = builder.build();
}
