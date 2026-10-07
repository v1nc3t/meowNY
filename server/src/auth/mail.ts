import nodemailer from 'nodemailer';

export type Mail = {
  to: string;
  subject: string;
  text: string;
};

export type Mailer = (mail: Mail) => Promise<void>;

type MailLog = {
  info: (message: string) => void;
};

/** Sends through SMTP. Defaults to the local Mailpit inbox. Logs only that a message went out. */
export function smtpMailer(logger?: MailLog): Mailer {
  const transport = nodemailer.createTransport({
    host: process.env.SMTP_HOST || '127.0.0.1',
    port: Number(process.env.SMTP_PORT || 1025),
    secure: false,
  });
  const from = process.env.SMTP_FROM || 'meowny@localhost';
  return async (mail) => {
    await transport.sendMail({ from, to: mail.to, subject: mail.subject, text: mail.text });
    logger?.info('mail sent');
  };
}
