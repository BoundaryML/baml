'use client';

import { Button } from '@baml/ui/button';
import { useEffect } from 'react';
import { useState } from 'react';

import { useDescribeMedia1599 } from '../../baml_client/react/hooks';
import { Image } from '../../baml_client/react/media';

export default function Test() {
  const describeMedia = useDescribeMedia1599();
  const [bamlImage, setImage] = useState<Image>();

  useEffect(() => {
    async function fetchIt() {
      const imageFromBase64 = await Image.fromUrlToBase64(
        'https://i.imgur.com/I7gJ3eY.png',
      );

      setImage(imageFromBase64);
    }
    fetchIt();
  }, []);

  return (
    <>
      <Button
        onClick={async () => {
          if (bamlImage) {
            const response = await describeMedia.mutate(
              bamlImage,
              'test',
              'test',
            );
            console.log(response);
          }
        }}
      >
        Test
      </Button>
      <div>{JSON.stringify(describeMedia.data)}</div>
    </>
  );
}
