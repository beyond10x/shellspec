declare module '@theme/ProductLandingPage' {
  import type {ReactNode} from 'react';
  import type {ProductLandingData} from '@beyond10x/docs-system/product-data';

  export default function ProductLandingPage(props: {landing: ProductLandingData}): ReactNode;
}
