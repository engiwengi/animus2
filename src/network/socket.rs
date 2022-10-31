use speedy::Readable;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tracing::trace;

use super::packet::{EncodedPacket, Packet};

pub const MAX_PACKET_LENGTH: usize = 5000;

pub struct Socket<T> {
    io: T,
    buffer: Vec<u8>,
}

impl<T> Socket<T> {
    pub fn new(io: T) -> Self {
        Self {
            io,
            buffer: Vec::with_capacity(MAX_PACKET_LENGTH),
        }
    }
}

impl<T> Socket<T>
where
    T: AsyncRead + Unpin,
{
    pub async fn ready(&mut self) -> Result<usize, std::io::Error> {
        trace!("Waiting for next packet");

        let length = self.io.read_u32_le().await? as usize;
        // TODO: Determine a good maximum packet size
        if length > MAX_PACKET_LENGTH {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "Packet length too long",
            ));
        }

        self.buffer.resize(length, 0);

        Ok(length)
    }

    pub async fn next<'d, 'a, P>(&'a mut self) -> Result<P, std::io::Error>
    where
        P: Packet + Readable<'d, speedy::LittleEndian>,
    {
        trace!("Reading packet of length {}", self.buffer.len());
        self.io.read_exact(&mut self.buffer).await?;
        trace!("Read packet: {:?}", &self.buffer);

        let packet = speedy::Readable::read_from_buffer_copying_data(&self.buffer)?;

        Ok(packet)
    }
}
impl<T> Socket<T>
where
    T: AsyncWrite + Unpin,
{
    pub async fn send(&mut self, packet: EncodedPacket) -> Result<(), std::io::Error> {
        trace!("Writing packet: {:?}", &packet);
        self.io.write_all(&packet.bytes).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;
    use crate::{
        chat::{entity::MessageKind, packet::SendMessage},
        network::packet::*,
    };

    #[rstest]
    #[case(SendMessage { kind: MessageKind::Shout, contents: "message".to_owned()})]
    #[tokio::test]
    async fn should_receive_packet<T>(#[case] packet: T)
    where
        T: PartialEq + std::fmt::Debug + Clone + TryFrom<ClientPacket>,
        ClientPacket: From<T>,
    {
        let any_packet = ClientPacket::from(packet.clone());
        let encoded_packet = speedy::Writable::write_to_vec(&any_packet).unwrap();
        let length = u32::to_le_bytes(encoded_packet.len() as u32);

        let reader = tokio_test::io::Builder::new()
            .read(&length)
            .read(&encoded_packet)
            .build();

        let mut socket = Socket::new(reader);

        socket.ready().await.unwrap();
        let decoded_any_packet = socket.next().await.unwrap();
        let decoded_packet = T::try_from(decoded_any_packet).unwrap_or_else(|_| unreachable!());

        assert_eq!(decoded_packet, packet);
    }

    #[rstest]
    #[tokio::test]
    async fn should_error_on_bad_packet() {
        let encoded_packet = vec![1, 2, 3, 4];
        let length = u32::to_le_bytes(encoded_packet.len() as u32);

        let reader = tokio_test::io::Builder::new()
            .read(&length)
            .read(&encoded_packet)
            .build();

        let mut socket = Socket::new(reader);

        socket.ready().await.unwrap();
        assert!(socket.next::<ClientPacket>().await.is_err());
    }
}
