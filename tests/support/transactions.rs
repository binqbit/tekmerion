use anchor_lang::error::ErrorCode;
use solana_instruction::Instruction;
use solana_instruction_error::InstructionError;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_signature::Signature;
use solana_signer::Signer;
use solana_transaction::Transaction;
use solana_transaction_error::TransactionError;

use super::{Actor, TestContext};

/// Execution facts obtained from the bank, plus its independently quoted fee.
#[derive(Debug)]
pub struct TransactionMetadata {
    pub signature: Signature,
    pub fee: u64,
    pub logs: Vec<String>,
    pub compute_units_consumed: u64,
}

#[derive(Debug)]
pub struct FailedTransactionMetadata {
    pub err: TransactionError,
    pub meta: TransactionMetadata,
}

pub enum ExpectedError {
    Protocol(tekmerion::errors::TekmerionError),
    Anchor(ErrorCode),
    Custom(u32),
    Native(InstructionError),
}
impl ExpectedError {
    fn instruction(self) -> InstructionError {
        match self {
            Self::Protocol(value) => InstructionError::Custom(value as u32 + 6000),
            Self::Anchor(value) => InstructionError::Custom(value as u32),
            Self::Custom(code) => InstructionError::Custom(code),
            Self::Native(error) => error,
        }
    }
}

impl TestContext {
    pub fn process_signed(
        &mut self,
        transaction: Transaction,
    ) -> Result<TransactionMetadata, Box<FailedTransactionMetadata>> {
        let mut meta = TransactionMetadata {
            signature: transaction.signatures.first().copied().unwrap_or_default(),
            fee: 0,
            logs: Vec::new(),
            compute_units_consumed: 0,
        };
        // BanksClient's metadata path skips the signature admission check.
        // Verify the original signed bytes before giving them to the bank.
        if let Err(err) = transaction.verify() {
            return Err(Box::new(FailedTransactionMetadata { err, meta }));
        }
        let fee = self
            .runtime
            .block_on(
                self.context
                    .banks_client
                    .get_fee_for_message(transaction.message.clone()),
            )
            .expect("quote bank transaction fee")
            .unwrap_or(0);
        let result = self
            .runtime
            .block_on(
                self.context
                    .banks_client
                    .process_transaction_with_metadata(transaction),
            )
            .expect("local bank transaction transport");
        if let Some(execution) = result.metadata {
            meta.fee = fee;
            meta.logs = execution.log_messages;
            meta.compute_units_consumed = execution.compute_units_consumed;
        }
        match result.result {
            Ok(()) => Ok(meta),
            Err(err) => Err(Box::new(FailedTransactionMetadata { err, meta })),
        }
    }

    /// Only the explicitly named actors sign; mutated metas cannot select a key.
    pub fn send(
        &mut self,
        instructions: &[Instruction],
        actors: &[Actor],
        additional: &[&Keypair],
    ) -> Result<TransactionMetadata, Box<FailedTransactionMetadata>> {
        self.context.last_blockhash = self
            .runtime
            .block_on(self.context.banks_client.get_latest_blockhash())
            .expect("latest bank blockhash");
        let mut transaction = self.signed_transaction(instructions, actors, additional);
        if self
            .transaction_status(&transaction.signatures[0])
            .is_some()
        {
            self.new_blockhash();
            transaction = self.signed_transaction(instructions, actors, additional);
        }
        self.process_signed(transaction)
    }

    fn signed_transaction(
        &self,
        instructions: &[Instruction],
        actors: &[Actor],
        additional: &[&Keypair],
    ) -> Transaction {
        let message = Message::new_with_blockhash(
            instructions,
            Some(&self.actor(Actor::Payer)),
            &self.latest_blockhash(),
        );
        let mut signers = vec![&self.keys[Actor::Payer as usize]];
        for actor in actors {
            let key = &self.keys[*actor as usize];
            if !signers.iter().any(|signer| signer.pubkey() == key.pubkey()) {
                signers.push(key);
            }
        }
        signers.extend_from_slice(additional);
        let tx = Transaction::new(&signers, message, self.latest_blockhash());
        assert!(
            wincode::serialize(&tx).unwrap().len() <= 1232,
            "transaction exceeds real legacy packet limit"
        );
        tx
    }

    pub fn ok(&mut self, instructions: &[Instruction], actors: &[Actor]) -> TransactionMetadata {
        self.send(instructions, actors, &[])
            .unwrap_or_else(|error| panic!("valid transaction rejected: {error:#?}"))
    }

    pub fn reject(
        &mut self,
        instructions: &[Instruction],
        actors: &[Actor],
        expected: ExpectedError,
    ) {
        assert_eq!(
            instructions.len(),
            1,
            "compound rollback checks must name the expected failing instruction index"
        );
        self.reject_at(instructions, actors, 0, expected);
    }

    pub fn reject_at(
        &mut self,
        instructions: &[Instruction],
        actors: &[Actor],
        index: u8,
        expected: ExpectedError,
    ) -> FailedTransactionMetadata {
        let mut addresses = self.protected.clone();
        for ix in instructions {
            addresses.extend(ix.accounts.iter().map(|m| m.pubkey));
        }
        addresses.remove(&self.actor(Actor::Payer));
        let before: Vec<_> = addresses
            .iter()
            .map(|id| (*id, self.get_account(id)))
            .collect();
        let payer_before = self.account(self.actor(Actor::Payer));
        let failure = self
            .send(instructions, actors, &[])
            .expect_err("hostile transaction must reject");
        let target = expected.instruction();
        assert_eq!(
            failure.err,
            TransactionError::InstructionError(index, target),
            "unexpected guard failed: {failure:#?}"
        );
        for (id, snapshot) in before {
            assert_eq!(
                self.get_account(&id),
                snapshot,
                "rejected transaction changed account {id}"
            );
        }
        let mut payer_after = self.account(self.actor(Actor::Payer));
        assert_eq!(
            payer_before.lamports - payer_after.lamports,
            failure.meta.fee,
            "rejected rent/payment survived"
        );
        payer_after.lamports = payer_before.lamports;
        assert_eq!(payer_before, payer_after);
        *failure
    }
}
