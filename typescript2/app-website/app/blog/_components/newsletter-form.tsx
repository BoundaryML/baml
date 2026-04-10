'use client';

import { useState } from 'react';

const INIT = 'INIT';
const SUBMITTING = 'SUBMITTING';
const ERROR = 'ERROR';
const SUCCESS = 'SUCCESS';
const formStates = [INIT, SUBMITTING, ERROR, SUCCESS] as const;
const formStyles = {
  buttonColor: '#000000',
  buttonFont: 'Inter',
  buttonFontColor: '#ffffff',
  buttonFontSizePx: 14,
  buttonText: 'Subscribe',
  formFont: 'Inter',
  formFontColor: '#000000',
  formFontSizePx: 14,
  formStyle: 'inline',
  id: 'cm3d7widq01su6gng1mc5iuvz',
  name: 'Default',
  placeholderText: 'Enter your email',
  successFont: 'Inter',
  successFontColor: '#000000',
  successFontSizePx: 16,
  successMessage: 'Get hyped for the latest BAML news!',
  userGroup: '',
};
const domain = 'app.loops.so';

interface Fields {
  [key: string]: string;
}

export function NewsletterForm() {
  const [email, setEmail] = useState('');
  const [formState, setFormState] = useState<(typeof formStates)[number]>(INIT);
  const [errorMessage, setErrorMessage] = useState('');
  const [fields, setFields] = useState<Fields>({});

  const resetForm = () => {
    setEmail('');
    setFormState(INIT);
    setErrorMessage('');
  };

  const hasRecentSubmission = () => {
    const time = new Date();
    const timestamp = time.valueOf();
    const previousTimestamp = localStorage.getItem('loops-form-timestamp');

    if (
      previousTimestamp &&
      Number(previousTimestamp) + 60 * 1000 > timestamp
    ) {
      setFormState(ERROR);
      setErrorMessage('Too many signups, please try again in a little while');
      return true;
    }

    localStorage.setItem('loops-form-timestamp', timestamp.toString());
    return false;
  };

  const handleSubmit = (event: React.FormEvent<HTMLFormElement>) => {
    event.preventDefault();

    if (formState !== INIT) return;
    if (!isValidEmail(email)) {
      setFormState(ERROR);
      setErrorMessage('Please enter a valid email');
      return;
    }
    if (hasRecentSubmission()) return;
    setFormState(SUBMITTING);

    const additionalFields = Object.entries(fields).reduce(
      (acc, [key, val]) => {
        if (val) {
          return acc + '&' + key + '=' + encodeURIComponent(val);
        }
        return acc;
      },
      '',
    );

    const formBody = `userGroup=${encodeURIComponent(
      formStyles.userGroup,
    )}&email=${encodeURIComponent(email)}&mailingLists=`;

    fetch(`https://${domain}/api/newsletter-form/${formStyles.id}`, {
      body: formBody + additionalFields,
      headers: {
        'Content-Type': 'application/x-www-form-urlencoded',
      },
      method: 'POST',
    })
      .then((res: any) => [res.ok, res.json(), res])
      .then(([ok, dataPromise, res]) => {
        if (ok) {
          resetForm();
          setFormState(SUCCESS);
        } else {
          dataPromise.then((data: any) => {
            setFormState(ERROR);
            setErrorMessage(data.message || res.statusText);
            localStorage.setItem('loops-form-timestamp', '');
          });
        }
      })
      .catch((error) => {
        setFormState(ERROR);
        if (error.message === 'Failed to fetch') {
          setErrorMessage(
            'Too many signups, please try again in a little while',
          );
        } else if (error.message) {
          setErrorMessage(error.message);
        }
        localStorage.setItem('loops-form-timestamp', '');
      });
  };

  const isInline = formStyles.formStyle === 'inline';

  switch (formState) {
    case SUCCESS:
      return (
        <div
          style={{
            alignItems: 'center',
            display: 'flex',
            justifyContent: 'flex-start',
            width: '100%',
          }}
        >
          <p
            style={{
              color: formStyles.successFontColor,
              fontFamily: `'${formStyles.successFont}', sans-serif`,
              fontSize: `${formStyles.successFontSizePx}px`,
            }}
          >
            {formStyles.successMessage}
          </p>
        </div>
      );
    case ERROR:
      return (
        <>
          <SignUpFormError />
          <BackButton />
        </>
      );
    default:
      return (
        <>
          <form
            onSubmit={handleSubmit}
            style={{
              alignItems: 'center',
              display: 'flex',
              flexDirection: isInline ? 'row' : 'column',
              justifyContent: 'flex-start',
              width: '100%',
            }}
          >
            <input
              name="email"
              onChange={(e) => setEmail(e.target.value)}
              placeholder={formStyles.placeholderText}
              required={true}
              style={{
                background: '#FFFFFF',
                border: '1px solid #D1D5DB',
                borderRadius: '6px',
                boxShadow: 'rgba(0, 0, 0, 0.05) 0px 1px 2px',
                boxSizing: 'border-box',
                color: formStyles.formFontColor,
                fontFamily: `'${formStyles.formFont}', sans-serif`,
                fontSize: `${formStyles.formFontSizePx}px`,
                margin: isInline ? '0px 10px 0px 0px' : '0px 0px 10px',
                maxWidth: '300px',
                minWidth: '100px',
                padding: '8px 12px',
                width: '100%',
              }}
              type="text"
              value={email}
            />
            <div
              aria-hidden="true"
              style={{ left: '-2024px', position: 'absolute' }}
            />
            <SignUpFormButton />
          </form>
        </>
      );
  }

  function SignUpFormError() {
    return (
      <div
        style={{
          alignItems: 'center',
          justifyContent: 'flex-start',
          width: '100%',
        }}
      >
        <p
          style={{
            color: 'rgb(185, 28, 28)',
            fontFamily: 'Inter, sans-serif',
            fontSize: '14px',
          }}
        >
          {errorMessage || 'Oops! Something went wrong, please try again'}
        </p>
      </div>
    );
  }

  function BackButton() {
    const [isHovered, setIsHovered] = useState(false);

    return (
      <button
        onClick={resetForm}
        onMouseOut={() => setIsHovered(false)}
        onMouseOver={() => setIsHovered(true)}
        style={{
          background: 'transparent',
          border: 'none',
          color: '#6b7280',
          cursor: 'pointer',
          font: '14px, Inter, sans-serif',
          margin: '10px auto',
          textAlign: 'center',
          textDecoration: isHovered ? 'underline' : 'none',
        }}
      >
        &larr; Back
      </button>
    );
  }

  function SignUpFormButton() {
    return (
      <button
        style={{
          alignItems: 'center',
          background: formStyles.buttonColor,
          border: 'none',
          borderRadius: '6px',
          boxShadow: '0px 1px 2px rgba(0, 0, 0, 0.05)',
          color: formStyles.buttonFontColor,
          cursor: 'pointer',
          flexDirection: 'row',
          fontFamily: `'${formStyles.buttonFont}', sans-serif`,
          fontSize: `${formStyles.buttonFontSizePx}px`,
          fontStyle: 'normal',
          fontWeight: 500,
          height: '38px',
          justifyContent: 'center',
          lineHeight: '20px',
          maxWidth: '300px',
          padding: '9px 17px',
          textAlign: 'center',
          whiteSpace: isInline ? 'nowrap' : 'normal',
          width: isInline ? 'min-content' : '100%',
        }}
        type="submit"
      >
        {formState === SUBMITTING ? 'Please wait...' : formStyles.buttonText}
      </button>
    );
  }
}

function isValidEmail(email: any) {
  return /.+@.+/.test(email);
}
